use std::sync::Arc;
use std::sync::atomic::{AtomicU64, Ordering};
use std::sync::{Mutex, MutexGuard};

use blackflower_acoustics::{AudibleSoundDelivery, AudibleVoiceDelivery};
use blackflower_ecs::{Error, PhaseId, RunError, TickDelta, World};

use crate::audio::{AudioCommand, PresentationAudioError, PresentationAudioState};
use crate::telemetry;
use crate::telemetry::FrameObservation;
use crate::{FrameIndex, PresentationPhase, PresentationPipeline, systems};

#[derive(Debug)]
struct ExecutionState {
    frame: AtomicU64,
    audio: Mutex<PresentationAudioState>,
}

/// Snapshot of the presentation execution visible to registered systems.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub struct FrameExecution {
    /// Frame currently being executed.
    pub frame: FrameIndex,
}

/// Shared read-only execution context that presentation systems may capture.
///
/// The delta for the active frame is available through each system's
/// [`SystemContext`](blackflower_ecs::SystemContext).
#[derive(Debug, Clone)]
pub struct FrameExecutionContext {
    state: Arc<ExecutionState>,
}

impl FrameExecutionContext {
    fn new() -> Self {
        Self {
            state: Arc::new(ExecutionState {
                frame: AtomicU64::new(FrameIndex::ZERO.get()),
                audio: Mutex::new(PresentationAudioState::default()),
            }),
        }
    }

    /// Return the frame prepared for the current pipeline invocation.
    #[must_use]
    pub fn current(&self) -> FrameExecution {
        FrameExecution {
            frame: FrameIndex::new(self.state.frame.load(Ordering::Acquire)),
        }
    }

    fn set(&self, execution: FrameExecution) {
        self.state
            .frame
            .store(execution.frame.get(), Ordering::Release);
    }

    fn audio(&self) -> Result<MutexGuard<'_, PresentationAudioState>, PresentationAudioError> {
        self.state
            .audio
            .lock()
            .map_err(|_error| PresentationAudioError::Unavailable)
    }

    pub(crate) fn reset_audio_transient(&self) -> Result<(), PresentationAudioError> {
        self.audio()?.reset_transient();
        Ok(())
    }

    pub(crate) fn update_audio_emitters(&self) -> Result<(), PresentationAudioError> {
        self.audio()?.update_emitters();
        Ok(())
    }

    pub(crate) fn build_audio_commands(&self) -> Result<(), PresentationAudioError> {
        self.audio()?.build();
        Ok(())
    }

    pub(crate) fn submit_audio_commands(&self) -> Result<(), PresentationAudioError> {
        self.audio()?.submit();
        Ok(())
    }
}

/// Failure while advancing a presentation frame.
#[derive(Debug, Clone, PartialEq, Eq, thiserror::Error)]
pub enum PresentationError {
    /// A registered ECS system failed.
    #[error(transparent)]
    Run(#[from] RunError),
    /// The monotonic presentation frame index exhausted its representation.
    #[error("presentation frame index overflow")]
    FrameIndexOverflow,
}

/// Dedicated ECS world for variable-step client presentation.
///
/// Construction registers the complete [`PresentationPipeline`]. The owner may
/// then register client-only components and systems through [`Self::ecs_mut`]
/// before advancing the pipeline once per displayed frame with [`Self::frame`].
///
/// Wall-clock pacing, input collection, prediction, reconciliation, transport,
/// and concrete output backends stay outside this type. Captured simulation
/// state is read-only; systems may mutate only presentation-owned state.
pub struct PresentationWorld {
    ecs: World,
    pipeline: PresentationPipeline,
    current_frame: FrameIndex,
    execution_context: FrameExecutionContext,
}

impl PresentationWorld {
    /// Create a single-threaded presentation world.
    pub fn new() -> Result<Self, Error> {
        Self::from_ecs(World::new()?)
    }

    /// Turn an existing, independently configured ECS world into a presentation world.
    pub fn from_ecs(mut ecs: World) -> Result<Self, Error> {
        let pipeline = PresentationPipeline::register(&mut ecs)?;
        let execution_context = FrameExecutionContext::new();
        systems::register(&mut ecs, pipeline, execution_context.clone())?;
        telemetry::describe_metrics();
        Ok(Self {
            ecs,
            pipeline,
            current_frame: FrameIndex::ZERO,
            execution_context,
        })
    }

    /// Return the underlying ECS world.
    #[must_use]
    pub const fn ecs(&self) -> &World {
        &self.ecs
    }

    /// Return the underlying ECS world for setup or direct client-only state access.
    #[must_use]
    pub const fn ecs_mut(&mut self) -> &mut World {
        &mut self.ecs
    }

    /// Return the registered presentation pipeline.
    #[must_use]
    pub const fn pipeline(&self) -> PresentationPipeline {
        self.pipeline
    }

    /// Return the world-bound handle for one presentation phase.
    #[must_use]
    pub const fn phase(&self, phase: PresentationPhase) -> PhaseId {
        self.pipeline.phase(phase)
    }

    /// Return the latest successfully completed presentation frame.
    #[must_use]
    pub const fn current_frame(&self) -> FrameIndex {
        self.current_frame
    }

    /// Return a context handle for systems that need the active frame index.
    #[must_use]
    pub fn execution_context(&self) -> FrameExecutionContext {
        self.execution_context.clone()
    }

    /// Queue one server-authorized remote sound for the next presentation frame.
    pub fn queue_audible_sound(
        &self,
        delivery: AudibleSoundDelivery,
    ) -> Result<(), PresentationAudioError> {
        self.execution_context.audio()?.queue_sound(delivery);
        Ok(())
    }

    /// Queue one server-gated physical-world voice packet for the next frame.
    pub fn queue_audible_voice(
        &self,
        delivery: AudibleVoiceDelivery,
    ) -> Result<(), PresentationAudioError> {
        self.execution_context.audio()?.queue_voice(delivery);
        Ok(())
    }

    /// Drain immutable audio commands after `SubmitAudioCommands` accepts them.
    pub fn drain_submitted_audio_commands(
        &self,
    ) -> Result<Vec<AudioCommand>, PresentationAudioError> {
        Ok(self.execution_context.audio()?.drain_submitted())
    }

    /// Advance the presentation pipeline by one frame using a validated delta.
    ///
    /// The frame index is committed only after every phase succeeds. In
    /// particular, a failure in `SubmitBackendCommands` prevents
    /// `CommitFrameHistory` from running and leaves the frame index unchanged.
    #[cfg_attr(
        feature = "tracing",
        tracing::instrument(
            target = "blackflower_world_presentation",
            name = "presentation_frame",
            level = "info",
            skip_all,
            fields(
                frame = tracing::field::Empty,
                delta_seconds = f64::from(delta.as_seconds()),
                result = tracing::field::Empty,
                reason = tracing::field::Empty,
            ),
        )
    )]
    pub fn frame(&mut self, delta: TickDelta) -> Result<bool, PresentationError> {
        let Some(next_frame) = self.current_frame.checked_next() else {
            telemetry::frame_rejected("frame_index_overflow");
            return Err(PresentationError::FrameIndexOverflow);
        };
        #[cfg(feature = "tracing")]
        tracing::Span::current().record("frame", next_frame.get());
        let previous_execution = self.execution_context.current();
        self.execution_context
            .set(FrameExecution { frame: next_frame });

        let observation = FrameObservation::start(delta);
        let run_result = self.ecs.progress(delta);
        let result = match run_result {
            Ok(should_continue) => {
                self.current_frame = next_frame;
                Ok(should_continue)
            }
            Err(error) => {
                self.execution_context.set(previous_execution);
                Err(PresentationError::Run(error))
            }
        };
        observation.finish(&result);

        #[cfg(feature = "profiling")]
        profiling::finish_frame!();

        result
    }
}
