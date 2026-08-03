use std::collections::BTreeMap;

use blackflower_acoustics::{AudibleSoundDelivery, AudibleVoiceDelivery};

use crate::FrameIndex;

/// Immutable backend-neutral audio command built by the presentation pipeline.
#[derive(Debug, Clone, PartialEq, Eq)]
#[allow(
    clippy::large_enum_variant,
    reason = "bounded inline Opus keeps presentation command transfer allocation-free after queue reservation"
)]
pub enum AudioCommand {
    /// Start a remote physical sound using only its authoritative delivery.
    PlayAudibleSound(AudibleSoundDelivery),
    /// Queue one server-gated exact-Opus live voice packet.
    PlayAudibleVoice(AudibleVoiceDelivery),
}

/// Idempotent frame-keyed audio command batch published to the audio worker.
#[derive(Debug, Clone, PartialEq, Eq)]
pub struct AudioCommandBatch {
    /// Presentation frame that produced the commands.
    pub frame: FrameIndex,
    /// Immutable commands in their canonical presentation order.
    pub commands: Vec<AudioCommand>,
}

/// Failure while accessing presentation-owned audio command state.
#[derive(Debug, Clone, Copy, PartialEq, Eq, thiserror::Error)]
pub enum PresentationAudioError {
    /// A previous panic poisoned the isolated command-state lock.
    #[error("presentation audio command state is unavailable")]
    Unavailable,
}

#[derive(Debug, Default)]
pub(crate) struct PresentationAudioState {
    incoming_sounds: Vec<AudibleSoundDelivery>,
    incoming_voices: Vec<AudibleVoiceDelivery>,
    emitters: Vec<AudioCommand>,
    built: Vec<AudioCommand>,
    submitted: BTreeMap<FrameIndex, Vec<AudioCommand>>,
    newest_published: Option<FrameIndex>,
}

impl PresentationAudioState {
    pub(crate) fn queue_sound(&mut self, delivery: AudibleSoundDelivery) {
        self.incoming_sounds.push(delivery);
    }

    pub(crate) fn queue_voice(&mut self, delivery: AudibleVoiceDelivery) {
        self.incoming_voices.push(delivery);
    }

    pub(crate) fn reset_transient(&mut self) {
        self.emitters.clear();
        self.built.clear();
    }

    pub(crate) fn update_emitters(&mut self) {
        self.emitters.extend(
            self.incoming_sounds
                .drain(..)
                .map(AudioCommand::PlayAudibleSound),
        );
        self.emitters.extend(
            self.incoming_voices
                .drain(..)
                .map(AudioCommand::PlayAudibleVoice),
        );
    }

    pub(crate) fn build(&mut self) {
        self.built.append(&mut self.emitters);
    }

    pub(crate) fn publish(&mut self, frame: FrameIndex) {
        if self.newest_published.is_some_and(|newest| newest >= frame) {
            self.built.clear();
            return;
        }
        self.newest_published = Some(frame);
        self.submitted
            .insert(frame, core::mem::take(&mut self.built));
    }

    pub(crate) fn drain_submitted(&mut self) -> Vec<AudioCommand> {
        core::mem::take(&mut self.submitted)
            .into_values()
            .flatten()
            .collect()
    }

    pub(crate) fn drain_submitted_batches(&mut self) -> Vec<AudioCommandBatch> {
        core::mem::take(&mut self.submitted)
            .into_iter()
            .map(|(frame, commands)| AudioCommandBatch { frame, commands })
            .collect()
    }
}
