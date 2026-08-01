use std::fs;
use std::path::Path;

use blackflower_animation::{Animation, Pose, SamplingContext, SamplingRatio, Skeleton};
use blackflower_animation_format::{AnimationContainer, SkeletonContainer};
use serde_json::{Value, json};
use tempfile::TempDir;

use super::{AnimationProfile, cook_animation, cook_skeleton};

const PROFILE: AnimationProfile = AnimationProfile {
    sampling_rate_hz: 0.0,
    iframe_interval_seconds: 10.0,
    optimize: true,
    optimization_tolerance: 0.001,
    optimization_distance: 0.1,
    root_motion_tolerance: 0.001,
};

#[test]
fn animation_profile_rejects_non_finite_and_non_positive_tolerances() {
    assert!(matches!(
        AnimationProfile {
            sampling_rate_hz: f32::NAN,
            ..PROFILE
        }
        .validate(),
        Err(super::Error::InvalidProfile)
    ));
    assert!(matches!(
        AnimationProfile {
            root_motion_tolerance: 0.0,
            ..PROFILE
        }
        .validate(),
        Err(super::Error::InvalidProfile)
    ));
}

#[test]
#[allow(
    clippy::too_many_lines,
    reason = "one end-to-end fixture keeps source mutation, cooking, and runtime proof together"
)]
fn gltf_cooks_deterministic_typed_assets_with_root_motion() -> Result<(), Box<dyn std::error::Error>>
{
    let directory = TempDir::new()?;
    let embedded_source = directory.path().join("embedded.gltf");
    let skeleton_source = directory.path().join("skeleton.glb");
    let animation_source = directory.path().join("animations.glb");
    let incompatible_source = directory.path().join("incompatible.glb");
    let fixture = Path::new(env!("CARGO_MANIFEST_DIR"))
        .join("../../vendor/ozz-animation/media/gltf/khronos/rigged_simple.gltf");
    let mut root: Value = serde_json::from_slice(&fs::read(fixture)?)?;
    root["animations"][0]["name"] = json!("Walk");
    root["animations"][0]["extras"]["blackflower"] = json!({
        "schema": 1,
        "loop": true,
        "additive": {"enabled": false, "reference": "animation"},
        "root_motion": {
            "enabled": true,
            "joint": "Bone.001",
            "translation_axes": ["x", "y", "z"],
            "rotation_axes": ["x", "y", "z"],
            "reference": "skeleton",
            "remove_from_pose": true,
            "loop_correction": true
        },
        "markers": [{"name": "middle", "time_seconds": 0.5}]
    });
    let mut raw_clip = root["animations"][0].clone();
    raw_clip["name"] = json!("Raw");
    raw_clip["extras"]["blackflower"] = json!({
        "schema": 1,
        "loop": false,
        "additive": {"enabled": false, "reference": "animation"},
        "root_motion": {"enabled": false},
        "markers": []
    });
    let mut additive_clip = root["animations"][0].clone();
    additive_clip["name"] = json!("Lean");
    additive_clip["extras"]["blackflower"] = json!({
        "schema": 1,
        "loop": false,
        "additive": {"enabled": true, "reference": "skeleton"},
        "root_motion": {"enabled": false},
        "markers": []
    });
    root["animations"]
        .as_array_mut()
        .ok_or("fixture animations are not an array")?
        .push(raw_clip);
    root["animations"]
        .as_array_mut()
        .ok_or("fixture animations are not an array")?
        .push(additive_clip);
    fs::write(&embedded_source, serde_json::to_vec(&root)?)?;
    let imported = gltf::import(&embedded_source)?;
    let buffer = imported.1.first().ok_or("fixture contains no buffer")?;
    let mut incompatible_root = root.clone();
    incompatible_root["nodes"][3]["matrix"][12] = json!(0.5);
    let incompatible_glb = build_glb(incompatible_root, &buffer.0)?;
    let glb = build_glb(root, &buffer.0)?;
    fs::write(&skeleton_source, &glb)?;
    fs::write(&animation_source, &glb)?;
    fs::write(&incompatible_source, incompatible_glb)?;

    let skeleton = cook_skeleton(&skeleton_source, "Armature")?;
    let animation = cook_animation(&animation_source, "Walk", &skeleton, PROFILE)?;
    let raw = cook_animation(&animation_source, "Raw", &skeleton, PROFILE)?;
    let additive = cook_animation(&animation_source, "Lean", &skeleton, PROFILE)?;
    assert_eq!(skeleton, cook_skeleton(&skeleton_source, "Armature")?);
    assert_eq!(
        animation,
        cook_animation(&animation_source, "Walk", &skeleton, PROFILE)?
    );
    assert_eq!(
        additive,
        cook_animation(&animation_source, "Lean", &skeleton, PROFILE)?
    );
    assert!(matches!(
        cook_animation(&incompatible_source, "Walk", &skeleton, PROFILE),
        Err(super::Error::AnimationSkeletonIdentityMismatch)
    ));

    let skeleton_container = SkeletonContainer::decode(&skeleton)?;
    let animation_container = AnimationContainer::decode(&animation)?;
    let additive_container = AnimationContainer::decode(&additive)?;
    assert_eq!(
        animation_container.skeleton_identity(),
        skeleton_container.identity()
    );
    assert_eq!(
        additive_container.skeleton_identity(),
        skeleton_container.identity()
    );
    assert!(animation_container.ozz_root_motion().is_some());
    assert!(additive_container.metadata().additive());
    assert!(additive_container.ozz_root_motion().is_none());

    let runtime_skeleton = Skeleton::from_bytes(&skeleton)?;
    let runtime_animation = Animation::from_bytes(&animation)?;
    let runtime_raw = Animation::from_bytes(&raw)?;
    let runtime_additive = Animation::from_bytes(&additive)?;
    assert_eq!(
        runtime_animation.skeleton_identity(),
        runtime_skeleton.skeleton_identity()
    );
    assert!(runtime_animation.looping());
    assert!(runtime_animation.root_motion().is_some());
    assert_eq!(runtime_animation.markers().markers().len(), 1);
    assert!(runtime_additive.additive());

    let joint = (0..runtime_skeleton.joint_count())
        .find(|joint| runtime_skeleton.joint_name(*joint) == Some("Bone.001"))
        .ok_or("cooked skeleton is missing Bone.001")?;
    let mut extracted_pose = Pose::new(&runtime_skeleton)?;
    let mut raw_pose = Pose::new(&runtime_skeleton)?;
    let mut extracted_context = SamplingContext::new(runtime_animation.track_count())?;
    let mut raw_context = SamplingContext::new(runtime_raw.track_count())?;
    let ratio = SamplingRatio::new(0.75)?;
    extracted_pose.sample(
        &runtime_skeleton,
        &runtime_animation,
        &mut extracted_context,
        ratio,
    )?;
    raw_pose.sample(&runtime_skeleton, &runtime_raw, &mut raw_context, ratio)?;
    let extracted = extracted_pose
        .local_transform(joint)
        .ok_or("missing extracted joint transform")?;
    let unmodified = raw_pose
        .local_transform(joint)
        .ok_or("missing raw joint transform")?;
    assert!(
        !extracted
            .translation
            .abs_diff_eq(unmodified.translation, 1.0e-5)
            || !extracted.rotation.abs_diff_eq(unmodified.rotation, 1.0e-5)
    );
    Ok(())
}

fn build_glb(mut root: Value, binary: &[u8]) -> Result<Vec<u8>, Box<dyn std::error::Error>> {
    root["buffers"][0]
        .as_object_mut()
        .ok_or("fixture buffer is not an object")?
        .remove("uri");
    let mut json = serde_json::to_vec(&root)?;
    while !json.len().is_multiple_of(4) {
        json.push(b' ');
    }
    let mut binary = binary.to_vec();
    while !binary.len().is_multiple_of(4) {
        binary.push(0);
    }
    let total = 12_usize
        .checked_add(8 + json.len())
        .and_then(|value| value.checked_add(8 + binary.len()))
        .ok_or("GLB fixture is too large")?;
    let mut output = Vec::with_capacity(total);
    output.extend_from_slice(b"glTF");
    output.extend_from_slice(&2_u32.to_le_bytes());
    output.extend_from_slice(&u32::try_from(total)?.to_le_bytes());
    output.extend_from_slice(&u32::try_from(json.len())?.to_le_bytes());
    output.extend_from_slice(&0x4e4f_534a_u32.to_le_bytes());
    output.extend_from_slice(&json);
    output.extend_from_slice(&u32::try_from(binary.len())?.to_le_bytes());
    output.extend_from_slice(&0x004e_4942_u32.to_le_bytes());
    output.extend_from_slice(&binary);
    Ok(output)
}
