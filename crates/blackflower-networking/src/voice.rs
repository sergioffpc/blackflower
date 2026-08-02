use std::collections::{BTreeMap, BTreeSet, VecDeque};

use crate::{PlayerId, VoiceStreamId};

/// Fixed Opus packet duration.
pub const VOICE_FRAME_MILLIS: u64 = 20;
/// Constrained voice target rate.
pub const VOICE_TARGET_KBPS: u32 = 24;
/// Maximum voice rate under available budget.
pub const VOICE_MAXIMUM_KBPS: u32 = 40;
/// Three-packet receive jitter interval.
pub const VOICE_JITTER_MILLIS: u64 = 60;
/// Maximum simultaneous audible deliveries per receiver.
pub const MAX_AUDIBLE_VOICES: usize = 4;
/// Maximum queued packets per voice stream.
pub const MAX_QUEUED_VOICE_PACKETS: usize = 3;

/// Authoritative routing scope bound to an authenticated voice stream.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum VoiceChannel {
    /// Physical attenuation and occlusion around the speaker.
    Proximity,
    /// Membership in the speaker's current squad.
    Squad,
    /// Membership in the speaker's current team.
    Team,
}

/// Invalid voice stream binding or routing result.
#[derive(Debug, Clone, Copy, PartialEq, Eq, thiserror::Error)]
pub enum VoiceError {
    /// A stream was rebound to a different authoritative channel.
    #[error("voice stream already has a different binding")]
    ConflictingBinding,
    /// The stream is not bound in the authenticated session.
    #[error("voice stream is not bound to the session")]
    UnboundStream,
    /// Routing selected more than four audible voices for one receiver.
    #[error("voice routing exceeds the four-delivery limit")]
    TooManyDeliveries,
}

/// Session-owned mapping from voice stream identity to routing scope.
#[derive(Debug, Default, Clone)]
pub struct VoiceBindings {
    channels: BTreeMap<VoiceStreamId, VoiceChannel>,
}

impl VoiceBindings {
    /// Create an empty authenticated voice binding table.
    #[must_use]
    pub const fn new() -> Self {
        Self {
            channels: BTreeMap::new(),
        }
    }

    /// Bind one stream once for this session.
    pub fn bind(&mut self, stream: VoiceStreamId, channel: VoiceChannel) -> Result<(), VoiceError> {
        match self.channels.get(&stream) {
            Some(existing) if *existing == channel => Ok(()),
            Some(_existing) => Err(VoiceError::ConflictingBinding),
            None => {
                self.channels.insert(stream, channel);
                Ok(())
            }
        }
    }

    /// Resolve the authoritative routing scope for an inbound packet.
    pub fn channel(&self, stream: VoiceStreamId) -> Result<VoiceChannel, VoiceError> {
        self.channels
            .get(&stream)
            .copied()
            .ok_or(VoiceError::UnboundStream)
    }
}

/// Deterministically limit and order receiver deliveries.
pub fn validate_voice_deliveries(
    deliveries: impl IntoIterator<Item = PlayerId>,
) -> Result<Vec<PlayerId>, VoiceError> {
    let unique = deliveries.into_iter().collect::<BTreeSet<_>>();
    if unique.len() > MAX_AUDIBLE_VOICES {
        Err(VoiceError::TooManyDeliveries)
    } else {
        Ok(unique.into_iter().collect())
    }
}

/// Per-stream latest jitter queue with no retransmission or fragmentation.
#[derive(Debug, Default, Clone)]
pub(crate) struct VoiceSendQueues {
    pub(crate) streams: BTreeMap<VoiceStreamId, VecDeque<Vec<u8>>>,
}

impl VoiceSendQueues {
    pub(crate) fn push(&mut self, stream: VoiceStreamId, bytes: Vec<u8>) {
        let queue = self.streams.entry(stream).or_default();
        if queue.len() == MAX_QUEUED_VOICE_PACKETS {
            let _expired = queue.pop_front();
        }
        queue.push_back(bytes);
    }

    pub(crate) fn pop_oldest(&mut self) -> Option<Vec<u8>> {
        let stream = self
            .streams
            .iter()
            .find_map(|(stream, queue)| (!queue.is_empty()).then_some(*stream))?;
        let packet = self.streams.get_mut(&stream)?.pop_front();
        if self.streams.get(&stream).is_some_and(VecDeque::is_empty) {
            let _empty = self.streams.remove(&stream);
        }
        packet
    }

    pub(crate) fn stream_count(&self) -> usize {
        self.streams
            .values()
            .filter(|queue| !queue.is_empty())
            .count()
    }
}
