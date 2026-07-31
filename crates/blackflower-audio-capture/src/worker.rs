use std::sync::Arc;
use std::sync::atomic::Ordering;

use blackflower_acoustics::{BandEnergy, EncodedVoice};
use blackflower_audio_voice::{Application, Channels, Encoder, FrameDuration, SampleRate};

use crate::ring::SampleRing;
use crate::stream::{CaptureSettings, CaptureState, VoiceActivation};
use crate::{Error, VoiceAcousticFrame};

const FRAME_SAMPLES: usize = 960;

/// One encoded 20 ms frame ready for a versioned voice-capture datagram.
#[derive(Debug, Clone, PartialEq, Eq)]
pub struct CapturedVoiceFrame {
    /// Client sample timestamp at 48 kHz.
    pub sample_timestamp: u64,
    /// Exact Opus packet.
    pub encoded: EncodedVoice,
    /// Quantized server/gameplay analysis hint; the server recomputes it from decoded Opus.
    pub energy: VoiceAcousticFrame,
}

/// Non-real-time mono/resample/VAD/Opus worker for one capture stream.
pub struct VoiceCaptureWorker {
    settings: CaptureSettings,
    source_rate: u32,
    channels: u16,
    ring: Arc<SampleRing>,
    state: Arc<CaptureState>,
    encoder: Encoder,
    channel_sum: f32,
    channel_index: u16,
    resample_accumulator: u64,
    frame: Vec<f32>,
    sequence: u32,
    sample_timestamp: u64,
}

impl VoiceCaptureWorker {
    pub(crate) fn new(
        settings: CaptureSettings,
        source_rate: u32,
        channels: u16,
        ring: Arc<SampleRing>,
        state: Arc<CaptureState>,
    ) -> Result<Self, Error> {
        let mut encoder = Encoder::new(SampleRate::Hz48K, Channels::Mono, Application::Voip)?;
        encoder.set_bitrate(settings.bitrate)?;
        encoder.set_complexity(settings.complexity)?;
        encoder.set_vbr(true)?;
        encoder.set_inband_fec(true)?;
        encoder.set_expected_packet_loss(10)?;
        encoder.set_dtx(false)?;
        Ok(Self {
            settings,
            source_rate,
            channels,
            ring,
            state,
            encoder,
            channel_sum: 0.0,
            channel_index: 0,
            resample_accumulator: 0,
            frame: Vec::with_capacity(FRAME_SAMPLES),
            sequence: 0,
            sample_timestamp: 0,
        })
    }

    /// Drain available input, appending zero or more active 20 ms frames.
    pub fn poll(&mut self, output: &mut Vec<CapturedVoiceFrame>) -> Result<usize, Error> {
        let initial = output.len();
        while let Some(sample) = self.ring.pop() {
            self.channel_sum += sample;
            self.channel_index = self.channel_index.saturating_add(1);
            if self.channel_index != self.channels {
                continue;
            }
            let mono = self.channel_sum / f32::from(self.channels);
            self.channel_sum = 0.0;
            self.channel_index = 0;
            self.resample_accumulator = self.resample_accumulator.saturating_add(48_000);
            while self.resample_accumulator >= u64::from(self.source_rate) {
                self.resample_accumulator -= u64::from(self.source_rate);
                self.frame.push(mono);
                if self.frame.len() == FRAME_SAMPLES {
                    self.finish_frame(output)?;
                }
            }
        }
        Ok(output.len().saturating_sub(initial))
    }

    fn finish_frame(&mut self, output: &mut Vec<CapturedVoiceFrame>) -> Result<(), Error> {
        let energy = analyze(&self.frame);
        let ptt = self.state.push_to_talk.load(Ordering::Acquire);
        let vad = energy.amplitude_q16 >= self.settings.vad_threshold_q16;
        let active = match self.settings.activation {
            VoiceActivation::PushToTalk => ptt,
            VoiceActivation::EnergyVad => vad,
            VoiceActivation::PushToTalkAndVad => ptt && vad,
        };
        if active {
            let mut packet = [0_u8; blackflower_acoustics::MAX_OPUS_PACKET_BYTES];
            let length = self
                .encoder
                .encode(FrameDuration::Ms20, &self.frame, &mut packet)?;
            let encoded =
                EncodedVoice::new(self.settings.stream, self.sequence, &packet[..length])?;
            output.push(CapturedVoiceFrame {
                sample_timestamp: self.sample_timestamp,
                encoded,
                energy,
            });
            self.sequence = self.sequence.saturating_add(1);
        }
        self.sample_timestamp = self.sample_timestamp.saturating_add(960);
        self.frame.clear();
        Ok(())
    }
}

#[allow(
    clippy::cast_possible_truncation,
    clippy::cast_sign_loss,
    reason = "finite normalized worker PCM is intentionally quantized to Q0.16 energy"
)]
pub(crate) fn analyze(samples: &[f32]) -> VoiceAcousticFrame {
    let mut low = 0.0_f64;
    let mut high = 0.0_f64;
    let mut total = 0.0_f64;
    for (index, sample) in samples.iter().enumerate() {
        let value = f64::from(*sample);
        total += value.abs();
        let previous = f64::from(*samples.get(index.saturating_sub(1)).unwrap_or(sample));
        high += (value - previous).abs() * 0.5;
        let begin = index.saturating_sub(3);
        let window = &samples[begin..=index];
        low += window
            .iter()
            .map(|sample| f64::from(*sample))
            .sum::<f64>()
            .abs()
            / f64::from(u32::try_from(window.len()).unwrap_or(1));
    }
    if total <= f64::EPSILON {
        return VoiceAcousticFrame {
            amplitude_q16: 0,
            bands: BandEnergy::SILENT,
        };
    }
    let amplitude =
        (total / f64::from(u32::try_from(samples.len()).unwrap_or(u32::MAX))).clamp(0.0, 1.0);
    low = low.min(total);
    high = high.min(total - low);
    let mid = total - low - high;
    let quantize = |value: f64| {
        (value / total * f64::from(u16::MAX))
            .round()
            .clamp(0.0, f64::from(u16::MAX)) as u16
    };
    VoiceAcousticFrame {
        amplitude_q16: (amplitude * f64::from(u16::MAX)).round() as u16,
        bands: BandEnergy([quantize(low), quantize(mid), quantize(high)]),
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::{CaptureSettings, CaptureStream, VoiceActivation};

    #[test]
    fn mock_microphone_resamples_encodes_and_obeys_ptt() -> Result<(), Error> {
        let settings = CaptureSettings {
            activation: VoiceActivation::PushToTalk,
            ..CaptureSettings::default()
        };
        let mut stream = CaptureStream::mock(settings, 48_000, 2)?;
        stream.set_push_to_talk(true);
        let mut worker = stream.take_worker()?;
        let samples = (0..1_920)
            .map(|index| if index % 2 == 0 { 0.25 } else { -0.25 })
            .collect::<Vec<_>>();
        stream.push_mock_interleaved(&samples)?;
        let mut output = Vec::new();
        assert_eq!(worker.poll(&mut output)?, 1);
        assert!(!output[0].encoded.payload().is_empty());
        Ok(())
    }

    #[test]
    fn energy_vad_drops_silence_and_keeps_the_sample_timeline() -> Result<(), Error> {
        let mut stream = CaptureStream::mock(CaptureSettings::default(), 48_000, 1)?;
        let mut worker = stream.take_worker()?;
        let mut output = Vec::new();
        stream.push_mock_interleaved(&[0.0; 960])?;
        assert_eq!(worker.poll(&mut output)?, 0);
        stream.push_mock_interleaved(&[0.25; 960])?;
        assert_eq!(worker.poll(&mut output)?, 1);
        assert_eq!(output[0].sample_timestamp, 960);
        assert!(output[0].energy.amplitude_q16 > 0);
        Ok(())
    }
}
