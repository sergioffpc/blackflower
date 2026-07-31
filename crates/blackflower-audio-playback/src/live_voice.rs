use blackflower_acoustics::{AudibleVoiceDelivery, PropagationDescriptor};
use blackflower_audio_voice::{Channels, Decoder, FrameDuration, SampleRate};

use crate::Error;

const VOICE_FRAME_SAMPLES: u64 = 960;

/// Metadata for one decoded or concealed mono 20 ms voice frame.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub struct DecodedVoiceFrame {
    /// Server sample on which this frame should play.
    pub play_sample: u64,
    /// Authoritative path parameters to apply before HRTF.
    pub propagation: PropagationDescriptor,
    /// Samples per channel written to the caller's fixed buffer.
    pub samples: usize,
    /// Whether PCM came from decoder packet-loss concealment.
    pub concealed: bool,
}

/// Bounded per-stream jitter buffer; decode and queue mutation stay off the callback.
pub struct RemoteVoiceJitterBuffer {
    capacity: usize,
    pending: Vec<AudibleVoiceDelivery>,
    decoder: Decoder,
    expected_sequence: Option<u32>,
}

impl RemoteVoiceJitterBuffer {
    /// Preallocate one bounded client voice queue.
    pub fn new(capacity: usize) -> Result<Self, Error> {
        if capacity == 0 {
            return Err(Error::InvalidField("live voice capacity"));
        }
        Ok(Self {
            capacity,
            pending: Vec::with_capacity(capacity),
            decoder: Decoder::new(SampleRate::Hz48K, Channels::Mono)?,
            expected_sequence: None,
        })
    }

    /// Insert one already authenticated and server-gated packet deterministically.
    pub fn push(&mut self, delivery: AudibleVoiceDelivery) -> Result<(), Error> {
        if self.pending.len() >= self.capacity {
            return Err(Error::ResourceLimit);
        }
        if self.pending.iter().any(|queued| {
            queued.encoded.stream == delivery.encoded.stream
                && queued.encoded.sequence == delivery.encoded.sequence
        }) {
            return Err(Error::InvalidField("duplicate live voice packet"));
        }
        if self
            .expected_sequence
            .is_some_and(|expected| delivery.encoded.sequence < expected)
        {
            return Err(Error::InvalidField("late live voice packet"));
        }
        self.pending.push(delivery);
        self.pending.sort_by_key(|queued| {
            (
                queued.play_sample,
                queued.encoded.stream,
                queued.encoded.sequence,
            )
        });
        Ok(())
    }

    /// Decode the next due frame, using PLC for a detected sequence gap.
    pub fn decode_due(
        &mut self,
        server_sample: u64,
        pcm: &mut [f32],
    ) -> Result<Option<DecodedVoiceFrame>, Error> {
        let Some(front) = self.pending.first() else {
            return Ok(None);
        };
        let expected = *self.expected_sequence.get_or_insert(front.encoded.sequence);
        if front.encoded.sequence > expected {
            let missing = u64::from(front.encoded.sequence - expected);
            let play_sample = front
                .play_sample
                .saturating_sub(missing.saturating_mul(VOICE_FRAME_SAMPLES));
            if play_sample > server_sample {
                return Ok(None);
            }
            let propagation = front.propagation;
            let samples = self.decoder.conceal(FrameDuration::Ms20, pcm)?;
            self.expected_sequence = Some(expected.saturating_add(1));
            return Ok(Some(DecodedVoiceFrame {
                play_sample,
                propagation,
                samples,
                concealed: true,
            }));
        }
        if front.play_sample > server_sample {
            return Ok(None);
        }
        let delivery = self.pending.remove(0);
        let samples = self.decoder.decode(delivery.encoded.payload(), pcm)?;
        self.expected_sequence = Some(delivery.encoded.sequence.saturating_add(1));
        Ok(Some(DecodedVoiceFrame {
            play_sample: delivery.play_sample,
            propagation: delivery.propagation,
            samples,
            concealed: false,
        }))
    }

    /// Number of packets retained in the bounded queue.
    #[must_use]
    pub fn len(&self) -> usize {
        self.pending.len()
    }

    /// Whether no packets are waiting.
    #[must_use]
    pub fn is_empty(&self) -> bool {
        self.pending.is_empty()
    }
}

#[cfg(test)]
mod tests {
    use blackflower_acoustics::{
        AcousticStructureVersion, BandEnergy, EncodedVoice, VoiceStreamId,
    };
    use blackflower_audio_voice::{Application, Encoder};

    use super::*;

    fn propagation(play_sample: u64) -> PropagationDescriptor {
        PropagationDescriptor {
            structure_version: AcousticStructureVersion(1),
            arrival_sample: play_sample,
            path_length_mm: 3_430,
            gain_db_q8: -256,
            band_gain: BandEnergy::UNITY,
            direction_q15: [i16::MAX, 0, 0],
            uncertainty_q16: 0,
            direct: true,
        }
    }

    fn packet(
        encoder: &mut Encoder,
        sequence: u32,
        play_sample: u64,
    ) -> Result<AudibleVoiceDelivery, Error> {
        let mut bytes = [0_u8; blackflower_acoustics::MAX_OPUS_PACKET_BYTES];
        let pcm = [0.1_f32; 960];
        let len = encoder.encode(FrameDuration::Ms20, &pcm, &mut bytes)?;
        Ok(AudibleVoiceDelivery {
            receiver_id: 1,
            play_sample,
            propagation: propagation(play_sample),
            encoded: EncodedVoice::new(VoiceStreamId(9), sequence, &bytes[..len])
                .map_err(|_error| Error::InvalidField("test voice"))?,
        })
    }

    #[test]
    fn jitter_buffer_schedules_packets_and_conceals_a_gap() -> Result<(), Error> {
        let mut encoder = Encoder::new(SampleRate::Hz48K, Channels::Mono, Application::Voip)?;
        let mut jitter = RemoteVoiceJitterBuffer::new(4)?;
        jitter.push(packet(&mut encoder, 2, 1_920)?)?;
        jitter.push(packet(&mut encoder, 0, 0)?)?;
        let mut pcm = [0.0_f32; 960];
        let first = jitter
            .decode_due(0, &mut pcm)?
            .ok_or(Error::InvalidField("missing first voice frame"))?;
        assert!(!first.concealed);
        let concealed = jitter
            .decode_due(1_920, &mut pcm)?
            .ok_or(Error::InvalidField("missing concealed voice frame"))?;
        assert!(concealed.concealed);
        let third = jitter
            .decode_due(1_920, &mut pcm)?
            .ok_or(Error::InvalidField("missing third voice frame"))?;
        assert!(!third.concealed);
        assert!(jitter.is_empty());
        Ok(())
    }
}
