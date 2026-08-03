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
    let hrtf = HrtfRuntime::new()?;
    let mut builder = TrackBuilder::new().sound_capacity(1);
    let parameters =
        builder.add_effect(HrtfBuilder::new(&hrtf, [1.0, 0.0, 0.0], Some(propagation))?);
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
