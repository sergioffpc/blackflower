use std::fs;

use blackflower_audio_spatial::{
    AcousticScene, BakedDataType, BakedDataVariation, PathBakeSettings, ProbeBatch,
    ReflectionsBakeSettings, Vec3A,
};
use blackflower_cooker_acoustics::{
    AcousticBakeProfile, AcousticMaterialDefinition, cook_probe_batch, cook_scene,
};

const FIXTURE: &str = include_str!("fixtures/room.gltf");

#[test]
fn scene_probes_and_baked_layers_form_a_vertical_slice() -> Result<(), Box<dyn std::error::Error>> {
    let directory = tempfile::tempdir()?;
    let source = directory.path().join("room.gltf");
    fs::write(&source, FIXTURE)?;
    let material = AcousticMaterialDefinition::new(
        "acoustics/materials/concrete",
        [0.2, 0.3, 0.4],
        0.1,
        [0.0; 3],
    )?;

    let scene = cook_scene(&source, std::slice::from_ref(&material))?;
    let decoded_scene = AcousticScene::from_bytes(scene.asset.bytes())?;
    assert_eq!(decoded_scene.vertex_count(), 4);
    assert_eq!(decoded_scene.triangle_count(), 2);
    assert_eq!(decoded_scene.material_count(), 1);

    let profile = AcousticBakeProfile {
        reflections: ReflectionsBakeSettings::new(64, 32, 2, 0.1, 0.1, 1, 1, 32, 0.1, 1)?,
        pathing: PathBakeSettings::new(4, 0.1, 0.5, 10.0, 20.0, 1)?,
    };
    let probes = cook_probe_batch(
        &source,
        &decoded_scene,
        "ground_floor_probes",
        2.0,
        0.5,
        profile,
    )?;
    let decoded_probes = ProbeBatch::from_bytes(probes.asset.bytes())?;
    assert_probe_batch(&decoded_probes);

    let moved_door = FIXTURE.replace(
        "\"translation\": [10.0, 0.0, 0.0]",
        "\"translation\": [20.0, 0.0, 0.0]",
    );
    fs::write(&source, moved_door)?;
    let unchanged_scene = cook_scene(&source, &[material])?;
    assert_eq!(unchanged_scene.source_hash, scene.source_hash);
    assert_eq!(unchanged_scene.asset.bytes(), scene.asset.bytes());
    Ok(())
}

fn assert_probe_batch(decoded_probes: &ProbeBatch) {
    assert_eq!(decoded_probes.zone(), "ground_floor");
    assert_eq!(decoded_probes.probes().len(), 9);
    assert_eq!(decoded_probes.layers().len(), 2);
    let expected_positions = [
        Vec3A::new(-2.0, 0.5, -2.0),
        Vec3A::new(-2.0, 0.5, 0.0),
        Vec3A::new(-2.0, 0.5, 2.0),
        Vec3A::new(0.0, 0.5, -2.0),
        Vec3A::new(0.0, 0.5, 0.0),
        Vec3A::new(0.0, 0.5, 2.0),
        Vec3A::new(2.0, 0.5, -2.0),
        Vec3A::new(2.0, 0.5, 0.0),
        Vec3A::new(2.0, 0.5, 2.0),
    ];
    for (probe, expected) in decoded_probes.probes().iter().zip(expected_positions) {
        assert!(probe.position().abs_diff_eq(expected, f32::EPSILON));
        assert!((probe.radius() - 2.0).abs() <= f32::EPSILON);
    }
    let reverb = decoded_probes.layers()[0];
    assert_eq!(reverb.identifier().data_type(), BakedDataType::Reflections);
    assert_eq!(reverb.identifier().variation(), BakedDataVariation::Reverb);
    assert!(reverb.byte_len() > 0);
    let pathing = decoded_probes.layers()[1];
    assert_eq!(pathing.identifier().data_type(), BakedDataType::Pathing);
    assert_eq!(
        pathing.identifier().variation(),
        BakedDataVariation::Dynamic
    );
    assert!(pathing.byte_len() > 0);
}
