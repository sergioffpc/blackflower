use std::sync::Arc;
use std::sync::atomic::{AtomicBool, Ordering};

use blackflower_acoustics::VoiceStreamId;
use cpal::traits::{DeviceTrait, HostTrait, StreamTrait};

use crate::ring::SampleRing;
use crate::{Error, VoiceCaptureWorker};

/// Voice activation policy evaluated on the worker thread.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum VoiceActivation {
    /// Send frames while push-to-talk is held.
    PushToTalk,
    /// Send frames whose energy exceeds the configured threshold.
    EnergyVad,
    /// Require both push-to-talk and energy VAD.
    PushToTalkAndVad,
}

/// Device-independent live voice capture settings.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub struct CaptureSettings {
    /// Host-scoped voice stream identifier.
    pub stream: VoiceStreamId,
    /// Preallocated input ring capacity in device frames.
    pub ring_capacity_frames: usize,
    /// Opus target bitrate.
    pub bitrate: u32,
    /// Opus complexity from zero through ten.
    pub complexity: u8,
    /// Energy threshold in Q0.16.
    pub vad_threshold_q16: u16,
    /// PTT/VAD policy.
    pub activation: VoiceActivation,
}

impl Default for CaptureSettings {
    fn default() -> Self {
        Self {
            stream: VoiceStreamId(1),
            ring_capacity_frames: 96_000,
            bitrate: 32_000,
            complexity: 10,
            vad_threshold_q16: 512,
            activation: VoiceActivation::EnergyVad,
        }
    }
}

#[derive(Debug)]
pub(crate) struct CaptureState {
    pub(crate) push_to_talk: AtomicBool,
    pub(crate) device_failed: AtomicBool,
}

/// Running CPAL input stream plus its preallocated producer state.
pub struct CaptureStream {
    stream: Option<cpal::Stream>,
    settings: CaptureSettings,
    sample_rate: u32,
    channels: u16,
    ring: Arc<SampleRing>,
    state: Arc<CaptureState>,
    worker_taken: bool,
}

impl CaptureStream {
    /// Open the default CPAL input device without starting it.
    pub fn open_default(settings: CaptureSettings) -> Result<Self, Error> {
        validate_settings(settings)?;
        let host = cpal::default_host();
        let device = host.default_input_device().ok_or(Error::NoInputDevice)?;
        let supported = device
            .default_input_config()
            .map_err(|error| Error::Device(error.to_string()))?;
        let sample_rate = supported.sample_rate();
        let channels = supported.channels();
        let capacity = settings
            .ring_capacity_frames
            .checked_mul(usize::from(channels))
            .ok_or(Error::InvalidSetting("ring_capacity_frames"))?;
        let ring = Arc::new(SampleRing::new(capacity));
        let state = Arc::new(CaptureState {
            push_to_talk: AtomicBool::new(false),
            device_failed: AtomicBool::new(false),
        });
        let config = supported.config();
        let stream = match supported.sample_format() {
            cpal::SampleFormat::F32 => build_f32(&device, config, &ring, &state)?,
            cpal::SampleFormat::I16 => build_i16(&device, config, &ring, &state)?,
            cpal::SampleFormat::U16 => build_u16(&device, config, &ring, &state)?,
            cpal::SampleFormat::I8
            | cpal::SampleFormat::I24
            | cpal::SampleFormat::I32
            | cpal::SampleFormat::I64
            | cpal::SampleFormat::U8
            | cpal::SampleFormat::U24
            | cpal::SampleFormat::U32
            | cpal::SampleFormat::U64
            | cpal::SampleFormat::F64
            | cpal::SampleFormat::DsdU8
            | cpal::SampleFormat::DsdU16
            | cpal::SampleFormat::DsdU32
            | _ => return Err(Error::UnsupportedSampleFormat),
        };
        Ok(Self {
            stream: Some(stream),
            settings,
            sample_rate,
            channels,
            ring,
            state,
            worker_taken: false,
        })
    }

    /// Construct a device-free stream for deterministic end-to-end tests.
    pub fn mock(settings: CaptureSettings, sample_rate: u32, channels: u16) -> Result<Self, Error> {
        validate_settings(settings)?;
        if sample_rate == 0 || channels == 0 {
            return Err(Error::InvalidSetting("mock format"));
        }
        let capacity = settings
            .ring_capacity_frames
            .checked_mul(usize::from(channels))
            .ok_or(Error::InvalidSetting("ring_capacity_frames"))?;
        Ok(Self {
            stream: None,
            settings,
            sample_rate,
            channels,
            ring: Arc::new(SampleRing::new(capacity)),
            state: Arc::new(CaptureState {
                push_to_talk: AtomicBool::new(false),
                device_failed: AtomicBool::new(false),
            }),
            worker_taken: false,
        })
    }

    /// Start callbacks for a real CPAL stream. Mock streams need no start.
    pub fn start(&self) -> Result<(), Error> {
        if let Some(stream) = &self.stream {
            stream
                .play()
                .map_err(|error| Error::Device(error.to_string()))?;
        }
        Ok(())
    }

    /// Pause callbacks for a real CPAL stream.
    pub fn pause(&self) -> Result<(), Error> {
        if let Some(stream) = &self.stream {
            stream
                .pause()
                .map_err(|error| Error::Device(error.to_string()))?;
        }
        Ok(())
    }

    /// Update PTT atomically without locking either audio thread.
    pub fn set_push_to_talk(&self, active: bool) {
        self.state.push_to_talk.store(active, Ordering::Release);
    }

    /// Whether the callback observed a CPAL stream failure.
    #[must_use]
    pub fn device_failed(&self) -> bool {
        self.state.device_failed.load(Ordering::Acquire)
    }

    /// Samples dropped because the fixed ring was full.
    #[must_use]
    pub fn dropped_samples(&self) -> u64 {
        self.ring.dropped()
    }

    /// Transfer the single ring consumer and initialize the Opus worker.
    pub fn take_worker(&mut self) -> Result<VoiceCaptureWorker, Error> {
        if self.worker_taken {
            return Err(Error::WorkerTaken);
        }
        let worker = VoiceCaptureWorker::new(
            self.settings,
            self.sample_rate,
            self.channels,
            Arc::clone(&self.ring),
            Arc::clone(&self.state),
        )?;
        self.worker_taken = true;
        Ok(worker)
    }

    /// Feed interleaved normalized samples into a mock stream.
    pub fn push_mock_interleaved(&self, samples: &[f32]) -> Result<(), Error> {
        if self.stream.is_some() {
            return Err(Error::NotMock);
        }
        for sample in samples {
            self.ring.push(sample.clamp(-1.0, 1.0));
        }
        Ok(())
    }
}

fn validate_settings(settings: CaptureSettings) -> Result<(), Error> {
    if settings.ring_capacity_frames < 1_920 {
        return Err(Error::InvalidSetting("ring_capacity_frames"));
    }
    if !(500..=512_000).contains(&settings.bitrate) {
        return Err(Error::InvalidSetting("bitrate"));
    }
    if settings.complexity > 10 {
        return Err(Error::InvalidSetting("complexity"));
    }
    Ok(())
}

fn build_f32(
    device: &cpal::Device,
    config: cpal::StreamConfig,
    ring: &Arc<SampleRing>,
    state: &Arc<CaptureState>,
) -> Result<cpal::Stream, Error> {
    build_stream::<f32>(device, config, ring, state, |sample| sample)
}

fn build_i16(
    device: &cpal::Device,
    config: cpal::StreamConfig,
    ring: &Arc<SampleRing>,
    state: &Arc<CaptureState>,
) -> Result<cpal::Stream, Error> {
    build_stream::<i16>(device, config, ring, state, |sample| {
        f32::from(sample) / f32::from(i16::MAX)
    })
}

fn build_u16(
    device: &cpal::Device,
    config: cpal::StreamConfig,
    ring: &Arc<SampleRing>,
    state: &Arc<CaptureState>,
) -> Result<cpal::Stream, Error> {
    build_stream::<u16>(device, config, ring, state, |sample| {
        (f32::from(sample) - 32_768.0) / 32_768.0
    })
}

fn build_stream<T>(
    device: &cpal::Device,
    config: cpal::StreamConfig,
    ring: &Arc<SampleRing>,
    state: &Arc<CaptureState>,
    convert: fn(T) -> f32,
) -> Result<cpal::Stream, Error>
where
    T: cpal::SizedSample + Copy + 'static,
{
    let callback_ring = Arc::clone(ring);
    let error_state = Arc::clone(state);
    device
        .build_input_stream::<T, _, _>(
            config,
            move |samples, _info| {
                for sample in samples {
                    callback_ring.push(convert(*sample));
                }
            },
            move |_error| {
                error_state.device_failed.store(true, Ordering::Release);
            },
            None,
        )
        .map_err(|error| Error::Device(error.to_string()))
}
