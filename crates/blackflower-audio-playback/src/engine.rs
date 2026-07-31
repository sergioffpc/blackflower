use std::collections::VecDeque;
use std::sync::Arc;

use blackflower_assets::AssetId;
use blackflower_audio_media::{
    AUDIO_SAMPLE_RATE, AudioAsset, AudioClip, AudioLibrary, AudioStream, LoopRegion, SoundEvent,
    Spatialization,
};
use kira::backend::DefaultBackend;
use kira::sound::static_sound::{StaticSoundData, StaticSoundHandle, StaticSoundSettings};
use kira::sound::streaming::{StreamingSoundData, StreamingSoundHandle};
use kira::sound::{PlaybackPosition, PlaybackState};
use kira::track::{TrackBuilder, TrackHandle};
use kira::{AudioManager, AudioManagerSettings, Frame, Tween};

use crate::decoder::KiraStreamDecoder;
use crate::hrtf::{DirectionHandle, HrtfBuilder};
use crate::{Error, INTERNAL_BUFFER_SIZE};
use blackflower_acoustics::PropagationDescriptor;

/// Opaque runtime voice identifier.
#[derive(Debug, Clone, Copy, PartialEq, Eq, PartialOrd, Ord, Hash)]
pub struct VoiceId(u64);

/// Device and voice-budget configuration.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub struct AudioEngineSettings {
    /// Global simultaneous voice limit.
    pub max_voices: usize,
}

impl Default for AudioEngineSettings {
    fn default() -> Self {
        Self { max_voices: 128 }
    }
}

/// Per-play runtime parameters not authored into `.bfsound`.
#[derive(Debug, Clone, Copy, PartialEq)]
pub struct PlaybackParams {
    /// Additional gain in decibels.
    pub gain_db: f32,
    /// Listener-relative source direction for HRTF.
    pub direction: [f32; 3],
    /// Optional current source distance in metres.
    pub distance_meters: Option<f32>,
    /// Optional server-authoritative direct/path parameters.
    pub propagation: Option<PropagationDescriptor>,
}

impl Default for PlaybackParams {
    fn default() -> Self {
        Self {
            gain_db: 0.0,
            direction: [0.0, 0.0, -1.0],
            distance_meters: None,
            propagation: None,
        }
    }
}

impl PlaybackParams {
    /// Derive client-safe HRTF/gain parameters from an authoritative delivery.
    #[must_use]
    pub fn from_propagation(propagation: PropagationDescriptor) -> Self {
        let mut direction = propagation
            .direction_q15
            .map(|value| f32::from(value) / f32::from(i16::MAX));
        if direction.iter().map(|value| value * value).sum::<f32>() <= f32::EPSILON {
            direction = [0.0, 0.0, -1.0];
        }
        Self {
            gain_db: 0.0,
            direction,
            distance_meters: None,
            propagation: Some(propagation),
        }
    }
}

/// Non-real-time playback lifecycle event.
#[derive(Debug, Clone, PartialEq, Eq)]
pub enum AudioEvent {
    /// A voice entered the mixer.
    Started(VoiceId),
    /// An older/lower-priority voice was stolen.
    Stolen(VoiceId),
    /// A voice was explicitly stopped.
    Stopped(VoiceId),
}

/// Kira-backed audio device and deterministic voice policy.
pub struct AudioEngine {
    manager: AudioManager<DefaultBackend>,
    two_d_track: TrackHandle,
    settings: AudioEngineSettings,
    next_voice: u64,
    next_order: u64,
    voices: Vec<ActiveVoice>,
    events: VecDeque<AudioEvent>,
}

impl AudioEngine {
    /// Open the default CPAL output device.
    pub fn new(settings: AudioEngineSettings) -> Result<Self, Error> {
        if settings.max_voices == 0 {
            return Err(Error::InvalidField("max_voices"));
        }
        let mut manager = AudioManager::<DefaultBackend>::new(AudioManagerSettings {
            internal_buffer_size: INTERNAL_BUFFER_SIZE,
            ..AudioManagerSettings::default()
        })
        .map_err(|error| Error::Device(error.to_string()))?;
        let two_d_track = manager
            .add_sub_track(TrackBuilder::new().sound_capacity(settings.max_voices))
            .map_err(|_error| Error::ResourceLimit)?;
        Ok(Self {
            manager,
            two_d_track,
            settings,
            next_voice: 1,
            next_order: 1,
            voices: Vec::with_capacity(settings.max_voices),
            events: VecDeque::new(),
        })
    }

    /// Resolve and play one source-less sound event.
    pub fn play(
        &mut self,
        library: &AudioLibrary,
        event_id: &AssetId,
        params: PlaybackParams,
    ) -> Result<VoiceId, Error> {
        self.retain_playing();
        validate_params(params)?;
        let event = library
            .event(event_id)
            .ok_or_else(|| Error::MissingAsset(event_id.clone()))?;
        let media = library
            .get(&event.media)
            .ok_or_else(|| Error::MissingAsset(event.media.clone()))?;
        self.reserve_voice(event)?;
        let gain_db = effective_gain(event, params)?;
        let voice_id = VoiceId(self.next_voice);
        self.next_voice = self.next_voice.wrapping_add(1).max(1);
        let order = self.next_order;
        self.next_order = self.next_order.wrapping_add(1);
        let (handle, track, direction) = match event.spatialization {
            Spatialization::TwoDimensional => (
                play_on_track(&mut self.two_d_track, media, gain_db, event.loop_region)?,
                None,
                None,
            ),
            Spatialization::Hrtf => {
                let mut builder = TrackBuilder::new()
                    .sound_capacity(1)
                    .persist_until_sounds_finish(true);
                let direction =
                    builder.add_effect(HrtfBuilder::new(params.direction, params.propagation));
                let mut track = self
                    .manager
                    .add_sub_track(builder)
                    .map_err(|_error| Error::ResourceLimit)?;
                let handle = play_on_track(&mut track, media, gain_db, event.loop_region)?;
                (handle, Some(track), Some(direction))
            }
        };
        self.voices.push(ActiveVoice {
            id: voice_id,
            order,
            priority: event.priority,
            group: event
                .concurrency
                .as_ref()
                .map(|policy| policy.group.clone()),
            handle,
            _track: track,
            direction,
        });
        self.events.push_back(AudioEvent::Started(voice_id));
        Ok(voice_id)
    }

    /// Stop a live voice.
    pub fn stop(&mut self, id: VoiceId) -> Result<(), Error> {
        let index = self
            .voices
            .iter()
            .position(|voice| voice.id == id)
            .ok_or(Error::UnknownVoice)?;
        let mut voice = self.voices.swap_remove(index);
        voice.handle.stop();
        self.events.push_back(AudioEvent::Stopped(id));
        Ok(())
    }

    /// Update listener-relative HRTF direction without locking the callback.
    pub fn set_direction(&mut self, id: VoiceId, direction: [f32; 3]) -> Result<(), Error> {
        validate_direction(direction)?;
        let voice = self
            .voices
            .iter_mut()
            .find(|voice| voice.id == id)
            .ok_or(Error::UnknownVoice)?;
        let handle = voice
            .direction
            .as_ref()
            .ok_or(Error::InvalidField("voice is not HRTF"))?;
        handle.set(direction);
        Ok(())
    }

    /// Publish new authoritative direct/path parameters without locking the callback.
    pub fn set_propagation(
        &mut self,
        id: VoiceId,
        propagation: PropagationDescriptor,
    ) -> Result<(), Error> {
        let voice = self
            .voices
            .iter_mut()
            .find(|voice| voice.id == id)
            .ok_or(Error::UnknownVoice)?;
        let handle = voice
            .direction
            .as_ref()
            .ok_or(Error::InvalidField("voice is not HRTF"))?;
        handle.set_propagation(propagation);
        Ok(())
    }

    /// Drain lifecycle events for observability outside the audio callback.
    pub fn drain_events(&mut self) -> impl Iterator<Item = AudioEvent> + '_ {
        self.events.drain(..)
    }

    fn reserve_voice(&mut self, event: &SoundEvent) -> Result<(), Error> {
        if let Some(policy) = &event.concurrency {
            let count = self
                .voices
                .iter()
                .filter(|voice| voice.group.as_deref() == Some(policy.group.as_str()))
                .count();
            if count >= usize::from(policy.max_voices) {
                return self.steal(event.priority, Some(&policy.group));
            }
        }
        if self.voices.len() >= self.settings.max_voices {
            return self.steal(event.priority, None);
        }
        Ok(())
    }

    fn steal(&mut self, priority: u8, group: Option<&str>) -> Result<(), Error> {
        let candidate = self
            .voices
            .iter()
            .enumerate()
            .filter(|(_, voice)| group.is_none_or(|group| voice.group.as_deref() == Some(group)))
            .min_by_key(|(_, voice)| (voice.priority, voice.order))
            .map(|(index, voice)| (index, voice.priority));
        let Some((index, old_priority)) = candidate else {
            return Err(Error::VoiceRejected);
        };
        if old_priority > priority {
            return Err(Error::VoiceRejected);
        }
        let mut voice = self.voices.swap_remove(index);
        voice.handle.stop();
        self.events.push_back(AudioEvent::Stolen(voice.id));
        Ok(())
    }

    fn retain_playing(&mut self) {
        self.voices.retain(|voice| !voice.handle.is_stopped());
    }
}

struct ActiveVoice {
    id: VoiceId,
    order: u64,
    priority: u8,
    group: Option<String>,
    handle: VoiceHandle,
    _track: Option<TrackHandle>,
    direction: Option<DirectionHandle>,
}

enum VoiceHandle {
    Static(StaticSoundHandle),
    Streaming(StreamingSoundHandle<blackflower_audio_media::Error>),
}

impl VoiceHandle {
    fn stop(&mut self) {
        match self {
            Self::Static(handle) => handle.stop(Tween::default()),
            Self::Streaming(handle) => handle.stop(Tween::default()),
        }
    }

    fn is_stopped(&self) -> bool {
        match self {
            Self::Static(handle) => handle.state() == PlaybackState::Stopped,
            Self::Streaming(handle) => handle.state() == PlaybackState::Stopped,
        }
    }
}

fn play_on_track(
    track: &mut TrackHandle,
    asset: &AudioAsset,
    gain_db: f32,
    event_loop: Option<LoopRegion>,
) -> Result<VoiceHandle, Error> {
    match asset {
        AudioAsset::Clip(clip) => {
            let loop_region = event_loop.or_else(|| clip.loop_region());
            let mut settings = StaticSoundSettings::new().volume(gain_db);
            if let Some(region) = loop_region {
                settings = settings.loop_region(
                    PlaybackPosition::Samples(
                        usize::try_from(region.start)
                            .map_err(|_error| Error::InvalidField("loop_region"))?,
                    )
                        ..PlaybackPosition::Samples(
                            usize::try_from(region.end)
                                .map_err(|_error| Error::InvalidField("loop_region"))?,
                        ),
                );
            }
            let data = StaticSoundData {
                sample_rate: AUDIO_SAMPLE_RATE,
                frames: clip_frames(clip),
                settings,
                slice: None,
            };
            track
                .play(data)
                .map(VoiceHandle::Static)
                .map_err(|_error| Error::ResourceLimit)
        }
        AudioAsset::Stream(stream) => play_stream(track, stream, gain_db, event_loop),
        AudioAsset::Event(_) => Err(Error::InvalidField("event media references another event")),
    }
}

fn play_stream(
    track: &mut TrackHandle,
    stream: &AudioStream,
    gain_db: f32,
    loop_region: Option<LoopRegion>,
) -> Result<VoiceHandle, Error> {
    let decoder = KiraStreamDecoder(stream.decoder()?);
    let mut data = StreamingSoundData::from_decoder(decoder).volume(gain_db);
    if let Some(region) = loop_region {
        data = data.loop_region(
            PlaybackPosition::Samples(
                usize::try_from(region.start)
                    .map_err(|_error| Error::InvalidField("loop_region"))?,
            )
                ..PlaybackPosition::Samples(
                    usize::try_from(region.end)
                        .map_err(|_error| Error::InvalidField("loop_region"))?,
                ),
        );
    }
    track
        .play(data)
        .map(VoiceHandle::Streaming)
        .map_err(|_error| Error::ResourceLimit)
}

fn clip_frames(clip: &AudioClip) -> Arc<[Frame]> {
    let scale = f32::from(i16::MAX);
    match clip.channels() {
        1 => clip
            .samples()
            .iter()
            .map(|sample| Frame::from_mono(f32::from(*sample) / scale))
            .collect::<Vec<_>>()
            .into(),
        2 => clip
            .samples()
            .chunks_exact(2)
            .map(|sample| Frame::new(f32::from(sample[0]) / scale, f32::from(sample[1]) / scale))
            .collect::<Vec<_>>()
            .into(),
        _ => Arc::from([]),
    }
}

fn validate_params(params: PlaybackParams) -> Result<(), Error> {
    if !params.gain_db.is_finite()
        || params
            .distance_meters
            .is_some_and(|distance| !distance.is_finite() || distance < 0.0)
    {
        return Err(Error::InvalidField("playback_params"));
    }
    validate_direction(params.direction)
}

fn validate_direction(direction: [f32; 3]) -> Result<(), Error> {
    let magnitude_squared = direction.iter().map(|value| value * value).sum::<f32>();
    if direction.iter().all(|value| value.is_finite()) && magnitude_squared > f32::EPSILON {
        Ok(())
    } else {
        Err(Error::InvalidField("direction"))
    }
}

fn effective_gain(event: &SoundEvent, params: PlaybackParams) -> Result<f32, Error> {
    let mut gain = event.gain_db + params.gain_db;
    if let (Some(attenuation), Some(distance)) = (event.attenuation, params.distance_meters) {
        let amplitude = if distance <= attenuation.min_distance {
            1.0
        } else if distance >= attenuation.max_distance {
            0.0
        } else {
            1.0 - (distance - attenuation.min_distance)
                / (attenuation.max_distance - attenuation.min_distance)
        };
        gain += if amplitude <= 0.0 {
            -120.0
        } else {
            20.0 * amplitude.log10()
        };
    }
    if gain.is_finite() {
        Ok(gain)
    } else {
        Err(Error::InvalidField("gain"))
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use blackflower_acoustics::{AcousticStructureVersion, BandEnergy};
    use blackflower_audio_media::{Concurrency, Spatialization};
    use kira::backend::mock::{MockBackend, MockBackendSettings};
    use std::str::FromStr;

    fn event(priority: u8, group: &str) -> Result<SoundEvent, Error> {
        Ok(SoundEvent {
            media: AssetId::from_str("audio/test")
                .map_err(|_error| Error::InvalidField("test asset ID"))?,
            gain_db: 0.0,
            priority,
            spatialization: Spatialization::TwoDimensional,
            loop_region: None,
            attenuation: None,
            concurrency: Some(Concurrency {
                group: group.to_owned(),
                max_voices: 1,
            }),
        })
    }

    #[test]
    fn attenuation_reaches_silence_at_max_distance() -> Result<(), Error> {
        let mut event = event(1, "test")?;
        event.attenuation = Some(blackflower_audio_media::Attenuation {
            min_distance: 1.0,
            max_distance: 10.0,
        });
        let gain = effective_gain(
            &event,
            PlaybackParams {
                distance_meters: Some(10.0),
                ..PlaybackParams::default()
            },
        )?;
        assert!((gain - -120.0).abs() < f32::EPSILON);
        Ok(())
    }

    #[test]
    fn kira_mock_accepts_blackflower_static_frames() -> Result<(), Error> {
        let mut manager = AudioManager::<MockBackend>::new(AudioManagerSettings {
            internal_buffer_size: INTERNAL_BUFFER_SIZE,
            backend_settings: MockBackendSettings {
                sample_rate: AUDIO_SAMPLE_RATE,
            },
            ..AudioManagerSettings::default()
        })
        .map_err(|()| Error::Device("mock backend failed".to_owned()))?;
        let data = StaticSoundData {
            sample_rate: AUDIO_SAMPLE_RATE,
            frames: Arc::from([Frame::ZERO; 16]),
            settings: StaticSoundSettings::default(),
            slice: None,
        };
        let handle = manager.play(data).map_err(|_error| Error::ResourceLimit)?;
        assert_ne!(handle.state(), PlaybackState::Stopped);
        Ok(())
    }

    #[test]
    fn kira_mock_processes_authoritative_effects_without_callback_setup() -> Result<(), Error> {
        let mut manager = AudioManager::<MockBackend>::new(AudioManagerSettings {
            internal_buffer_size: INTERNAL_BUFFER_SIZE,
            backend_settings: MockBackendSettings {
                sample_rate: AUDIO_SAMPLE_RATE,
            },
            ..AudioManagerSettings::default()
        })
        .map_err(|()| Error::Device("mock backend failed".to_owned()))?;
        let propagation = PropagationDescriptor {
            structure_version: AcousticStructureVersion(1),
            arrival_sample: 960,
            path_length_mm: 3_430,
            gain_db_q8: -3 * 256,
            band_gain: BandEnergy([u16::MAX, 40_000, 20_000]),
            direction_q15: [i16::MAX, 0, 0],
            uncertainty_q16: 0,
            direct: true,
        };
        let mut builder = TrackBuilder::new().sound_capacity(1);
        let parameters = builder.add_effect(HrtfBuilder::new([1.0, 0.0, 0.0], Some(propagation)));
        let mut track = manager
            .add_sub_track(builder)
            .map_err(|_error| Error::ResourceLimit)?;
        let data = StaticSoundData {
            sample_rate: AUDIO_SAMPLE_RATE,
            frames: Arc::from([Frame::from_mono(0.25); INTERNAL_BUFFER_SIZE * 2]),
            settings: StaticSoundSettings::default(),
            slice: None,
        };
        let _handle = track.play(data).map_err(|_error| Error::ResourceLimit)?;
        manager.backend_mut().on_start_processing();
        manager.backend_mut().process();
        parameters.set_propagation(propagation);
        manager.backend_mut().on_start_processing();
        manager.backend_mut().process();
        Ok(())
    }
}
