use blackflower_audio::{
    AudioSettings, BinauralParams, Context, Error, STEAM_AUDIO_VERSION, TailState, Vec3A,
};

const FRAME_SIZE: usize = 256;
const FRAME_SIZE_U32: u32 = 256;

#[test]
fn bindings_report_the_pinned_steam_audio_version() {
    assert_eq!(STEAM_AUDIO_VERSION, (4, 8, 1));
    assert_send::<blackflower_audio::BinauralEffect>();
}

#[test]
fn default_hrtf_spatializes_a_mono_impulse() -> Result<(), Error> {
    if !native_sdk_is_configured() {
        return Ok(());
    }

    let settings = test_settings()?;
    let mut context = Context::new()?;
    let hrtf = context.create_default_hrtf(settings)?;
    let mut effect = context.create_binaural_effect(&hrtf)?;
    let mut input = [0.0; FRAME_SIZE];
    input[0] = 1.0;
    let mut left = [0.0; FRAME_SIZE];
    let mut right = [0.0; FRAME_SIZE];

    let state = effect.process_mono(
        BinauralParams::new(Vec3A::X)?,
        &input,
        &mut left,
        &mut right,
    )?;

    assert!(matches!(state, TailState::Remaining | TailState::Complete));
    assert!(left.iter().chain(&right).all(|sample| sample.is_finite()));
    let energy: f32 = left.iter().chain(&right).map(|sample| sample.abs()).sum();
    assert!(energy > f32::EPSILON);
    Ok(())
}

#[test]
fn safe_api_rejects_invalid_directions_and_frame_lengths() -> Result<(), Error> {
    assert!(matches!(
        BinauralParams::new(Vec3A::ZERO),
        Err(Error::InvalidDirection)
    ));
    if !native_sdk_is_configured() {
        return Ok(());
    }

    let settings = test_settings()?;
    let mut context = Context::new()?;
    let hrtf = context.create_default_hrtf(settings)?;
    let mut effect = context.create_binaural_effect(&hrtf)?;
    let input = [0.0; FRAME_SIZE - 1];
    let mut left = [0.0; FRAME_SIZE];
    let mut right = [0.0; FRAME_SIZE];
    let result = effect.process_mono(
        BinauralParams::new(Vec3A::NEG_Z)?,
        &input,
        &mut left,
        &mut right,
    );

    assert!(matches!(
        result,
        Err(Error::FrameLength {
            buffer: "input",
            expected: FRAME_SIZE,
            actual
        }) if actual == FRAME_SIZE - 1
    ));
    Ok(())
}

fn test_settings() -> Result<AudioSettings, Error> {
    AudioSettings::new(48_000, FRAME_SIZE_U32)
}

fn native_sdk_is_configured() -> bool {
    std::env::var_os("BLACKFLOWER_STEAM_AUDIO_LIBRARY").is_some()
}

const fn assert_send<T: Send>() {}
