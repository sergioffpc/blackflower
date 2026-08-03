use blackflower_audio_spatial::{
    AcousticMaterial, AcousticScene, AcousticTriangle, AudioSettings, BinauralParams, Context,
    EMBREE_VERSION, Error, PathBakeSettings, ProbeBatch, ProbeVolumeTransform, RayTracerBackend,
    ReflectionSimulator, ReflectionsBakeSettings, STEAM_AUDIO_EMBREE_ENABLED, STEAM_AUDIO_VERSION,
    TailState, Vec3A,
};

const FRAME_SIZE: usize = 256;
const FRAME_SIZE_U32: u32 = 256;

#[test]
fn bindings_report_the_pinned_steam_audio_version() {
    assert_eq!(STEAM_AUDIO_VERSION, (4, 8, 1));
    assert_eq!(EMBREE_VERSION, (4, 4, 1));
    assert_send::<blackflower_audio_spatial::BinauralEffect>();
}

#[test]
fn context_selects_embree_only_when_compiled_for_the_target() -> Result<(), Error> {
    let mut context = Context::new()?;
    let expected = if STEAM_AUDIO_EMBREE_ENABLED {
        RayTracerBackend::Embree
    } else {
        RayTracerBackend::BuiltIn
    };
    assert_eq!(context.ray_tracer_backend(), expected);
    let selected_scene = context.create_scene()?;
    assert_eq!(selected_scene.ray_tracer_backend(), expected);
    if expected == RayTracerBackend::Embree {
        assert!(matches!(
            selected_scene.to_acoustic_asset(0, 0, 0),
            Err(Error::SceneSerializationRequiresBuiltIn)
        ));
    }
    let serializable_scene = context.create_serializable_scene()?;
    assert_eq!(
        serializable_scene.ray_tracer_backend(),
        RayTracerBackend::BuiltIn
    );

    let portable = Context::with_ray_tracer(RayTracerBackend::BuiltIn)?;
    assert_eq!(portable.ray_tracer_backend(), RayTracerBackend::BuiltIn);
    if !STEAM_AUDIO_EMBREE_ENABLED {
        assert!(matches!(
            Context::with_ray_tracer(RayTracerBackend::Embree),
            Err(Error::RayTracerUnavailable {
                backend: RayTracerBackend::Embree
            })
        ));
    }
    Ok(())
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
fn rigid_sub_scene_instance_updates_with_one_parent_commit() -> Result<(), Error> {
    let mut context = Context::new()?;
    let mut parent = context.create_scene()?;
    let mut prefab = context.create_scene()?;
    let material = AcousticMaterial::new([0.1; 3], 0.05, [0.01; 3])?;
    let mut mesh = prefab.create_static_mesh(
        &[Vec3A::ZERO, Vec3A::X, Vec3A::Y],
        &[AcousticTriangle::new(0, 1, 2)],
        &[0],
        &[material],
    )?;
    mesh.add();
    prefab.commit();
    let identity = [
        [1.0, 0.0, 0.0, 0.0],
        [0.0, 1.0, 0.0, 0.0],
        [0.0, 0.0, 1.0, 0.0],
        [0.0, 0.0, 0.0, 1.0],
    ];
    let mut instance = parent.create_instanced_mesh(&prefab, identity)?;
    instance.add();
    let mut translated = identity;
    translated[3][0] = 2.0;
    instance.update_transform(translated)?;
    parent.commit();
    assert!(instance.is_added());
    Ok(())
}

#[test]
fn reflection_updates_dirty_only_adjacent_zones_and_coalesce_commit() -> Result<(), Error> {
    let mut context = Context::new()?;
    let mut scene = context.create_scene()?;
    let mut reflections = ReflectionSimulator::new(8, 200)?;
    reflections.mark_dirty([1, 2]);
    reflections.mark_dirty([2, 3]);
    let Some(update) = reflections.commit(&mut scene) else {
        return Err(Error::InvalidReflectionSettings);
    };
    assert_eq!(update.dirty_zones, [1, 2, 3]);
    assert_eq!(update.crossfade_ms, 200);
    assert!(reflections.commit(&mut scene).is_none());
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

#[test]
fn cooked_scene_and_probe_assets_round_trip_through_steam_audio() -> Result<(), Error> {
    let mut context = Context::new()?;
    let mut scene = context.create_serializable_scene()?;
    assert_eq!(scene.ray_tracer_backend(), RayTracerBackend::BuiltIn);
    let material = AcousticMaterial::new([0.2, 0.3, 0.4], 0.1, [0.0; 3])?;
    let mut floor = scene.create_static_mesh(
        &[
            Vec3A::new(-2.0, 0.0, -2.0),
            Vec3A::new(2.0, 0.0, -2.0),
            Vec3A::new(2.0, 0.0, 2.0),
            Vec3A::new(-2.0, 0.0, 2.0),
        ],
        &[
            AcousticTriangle::new(0, 2, 1),
            AcousticTriangle::new(0, 3, 2),
        ],
        &[0, 0],
        &[material],
    )?;
    floor.add();
    scene.commit();

    let scene_asset = scene.to_acoustic_asset(4, 2, 1)?;
    let decoded_scene = AcousticScene::from_bytes(scene_asset.bytes())?;
    let loaded_scene = context.load_acoustic_scene(&decoded_scene)?;
    let volume = ProbeVolumeTransform::new([
        [4.0, 0.0, 0.0, 0.0],
        [0.0, 2.0, 0.0, 1.0],
        [0.0, 0.0, 4.0, 0.0],
        [0.0, 0.0, 0.0, 1.0],
    ])?;
    let reflections = ReflectionsBakeSettings::new(64, 32, 2, 0.1, 0.1, 1, 1, 32, 0.1, 1)?;
    let pathing = PathBakeSettings::new(4, 0.1, 0.5, 10.0, 20.0, 1)?;
    let probes = context.bake_uniform_floor_probe_batch(
        &loaded_scene,
        "ground_floor",
        volume,
        2.0,
        0.5,
        reflections,
        pathing,
    )?;
    let decoded_probes = ProbeBatch::from_bytes(probes.bytes())?;
    assert!(!decoded_probes.probes().is_empty());
    assert_eq!(decoded_probes.layers().len(), 2);
    let native = context.load_probe_batch(&decoded_probes)?;
    assert_eq!(native.probe_count(), decoded_probes.probes().len());
    Ok(())
}

fn test_settings() -> Result<AudioSettings, Error> {
    AudioSettings::new(48_000, FRAME_SIZE_U32)
}

const fn assert_send<T: Send>() {}
