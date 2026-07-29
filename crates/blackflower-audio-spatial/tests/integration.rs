use blackflower_audio_spatial::{
    AcousticMaterial, AcousticTriangle, AudioSettings, BinauralParams, Context, Error,
    STEAM_AUDIO_VERSION, TailState, Vec3A,
};

const FRAME_SIZE: usize = 256;
const FRAME_SIZE_U32: u32 = 256;

#[test]
fn bindings_report_the_pinned_steam_audio_version() {
    assert_eq!(STEAM_AUDIO_VERSION, (4, 8, 1));
    assert_send::<blackflower_audio_spatial::BinauralEffect>();
}

#[test]
fn default_hrtf_spatializes_a_mono_impulse() -> Result<(), Error> {
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

#[test]
fn default_scene_commits_static_acoustic_geometry() -> Result<(), Error> {
    let mut context = Context::new()?;
    let mut scene = context.create_scene()?;
    let material = AcousticMaterial::new([0.1, 0.2, 0.3], 0.05, [0.01, 0.02, 0.03])?;
    let mut mesh = scene.create_static_mesh(
        &[Vec3A::ZERO, Vec3A::X, Vec3A::Y],
        &[AcousticTriangle::new(0, 1, 2)],
        &[0],
        &[material],
    )?;

    mesh.add();
    assert!(mesh.is_added());
    scene.commit();
    mesh.remove();
    assert!(!mesh.is_added());
    scene.commit();
    Ok(())
}

#[test]
fn acoustic_scene_rejects_invalid_materials_and_geometry() -> Result<(), Error> {
    assert!(matches!(
        AcousticMaterial::new([0.1, f32::NAN, 0.3], 0.05, [0.01, 0.02, 0.03]),
        Err(Error::InvalidAcousticMaterial)
    ));

    let mut context = Context::new()?;
    let mut scene = context.create_scene()?;
    let material = AcousticMaterial::new([0.1, 0.2, 0.3], 0.05, [0.01, 0.02, 0.03])?;
    assert!(matches!(
        scene.create_static_mesh(
            &[Vec3A::ZERO, Vec3A::X, Vec3A::Y],
            &[AcousticTriangle::new(0, 1, 3)],
            &[0],
            &[material],
        ),
        Err(Error::InvalidSceneGeometry)
    ));
    Ok(())
}

fn test_settings() -> Result<AudioSettings, Error> {
    AudioSettings::new(48_000, FRAME_SIZE_U32)
}

const fn assert_send<T: Send>() {}
