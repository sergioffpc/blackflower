use blackflower_animation::{
    AimIk, Animation, AnimationGraph, AnimationMarker, AnimationSet, AnimationState, BlendLayer,
    Error, MarkerTrack, Pose, RootMotionTransform, SamplingContext, SamplingRatio, Skeleton,
    SkeletonIdentity, TwoBoneIk, ozz_version, simd_implementation,
};
use blackflower_animation_format::{
    AnimationContainer, ClipMetadata, OzzVersion, SkeletonContainer,
};
use glam::Vec3;

const SKELETON_OZZ: &[u8] = include_bytes!("../vendor/ozz-animation/media/bin/baked_skeleton.ozz");
const ANIMATION_OZZ: &[u8] =
    include_bytes!("../vendor/ozz-animation/media/bin/baked_animation.ozz");
const OTHER_SKELETON_OZZ: &[u8] =
    include_bytes!("../vendor/ozz-animation/media/bin/robot_skeleton.ozz");
const ROOT_MOTION_OZZ: &[u8] =
    include_bytes!("../vendor/ozz-animation/media/bin/pab_walk_motion_track.ozz");
const VERSION: OzzVersion = OzzVersion::new(0, 16, 0);
const SKELETON_IDENTITY: SkeletonIdentity = SkeletonIdentity::from_bytes([
    0x17, 0xb4, 0x9c, 0x4b, 0xf2, 0x33, 0x22, 0x2e, 0x60, 0x03, 0x58, 0x40, 0x5b, 0xce, 0xe1, 0xd5,
    0x26, 0xd8, 0x40, 0xee, 0x0f, 0x79, 0x56, 0xa6, 0x81, 0x20, 0x92, 0xcf, 0x93, 0xea, 0x06, 0xfb,
]);
const OTHER_SKELETON_IDENTITY: SkeletonIdentity = SkeletonIdentity::from_bytes([
    0x57, 0x44, 0x12, 0x1f, 0xb4, 0xf8, 0x98, 0x85, 0xc6, 0xdb, 0x77, 0xa1, 0x55, 0xfb, 0x24, 0x1b,
    0xcc, 0xa2, 0xde, 0xac, 0xd6, 0xe3, 0xa0, 0xb9, 0x44, 0x1f, 0xc1, 0xaa, 0x03, 0xfb, 0x36, 0xcb,
]);

fn load_skeleton(raw: &[u8], identity: SkeletonIdentity) -> Result<Skeleton, Error> {
    let asset = SkeletonContainer::encode(VERSION, identity, raw)
        .map_err(|_error| Error::InvalidSkeletonArchive)?;
    Skeleton::from_bytes(&asset)
}

fn load_animation(root_motion: bool) -> Result<Animation, Error> {
    load_animation_for(SKELETON_IDENTITY, root_motion)
}

fn load_animation_for(
    skeleton_identity: SkeletonIdentity,
    root_motion: bool,
) -> Result<Animation, Error> {
    let metadata = ClipMetadata::new("Take 001", true, false, [])
        .map_err(|_error| Error::InvalidAnimationArchive)?;
    let asset = AnimationContainer::encode(
        VERSION,
        skeleton_identity,
        ANIMATION_OZZ,
        &metadata,
        root_motion.then_some(ROOT_MOTION_OZZ),
    )
    .map_err(|_error| Error::InvalidAnimationArchive)?;
    Animation::from_bytes(&asset)
}

#[test]
fn bindings_report_the_pinned_ozz_version_and_simd_backend() {
    assert_eq!(ozz_version(), (0, 16, 0));
    assert!(!simd_implementation().is_empty());
}

#[test]
fn runtime_archives_expose_skeleton_and_clip_metadata() -> Result<(), Error> {
    let skeleton = load_skeleton(SKELETON_OZZ, SKELETON_IDENTITY)?;
    let animation = load_animation(false)?;

    assert!(skeleton.joint_count() > 1);
    assert!(skeleton.joint_name(0).is_some_and(|name| !name.is_empty()));
    assert_eq!(skeleton.joint_parent(0), Some(None));
    assert_eq!(animation.track_count(), skeleton.joint_count());
    assert!(animation.duration().is_finite());
    assert!(animation.duration() > 0.0);
    Ok(())
}

#[test]
fn root_motion_tracks_sample_and_cross_a_loop() -> Result<(), Error> {
    let animation = load_animation(true)?;
    let Some(track) = animation.root_motion() else {
        return Err(Error::InvalidRootMotionArchive);
    };
    let start = track.sample(SamplingRatio::new(0.0)?)?;
    let end = track.sample(SamplingRatio::new(1.0)?)?;
    let previous = track.sample(SamplingRatio::new(0.25)?)?;
    let current = track.sample(SamplingRatio::new(0.75)?)?;
    let normal = track.delta(SamplingRatio::new(0.25)?, SamplingRatio::new(0.75)?, 0)?;
    assert_transform_close(normal, relative_motion(previous, current));

    let previous = track.sample(SamplingRatio::new(0.75)?)?;
    let current = track.sample(SamplingRatio::new(0.25)?)?;
    let wrapped = track.delta(SamplingRatio::new(0.75)?, SamplingRatio::new(0.25)?, 1)?;
    let expected = compose_motion(
        relative_motion(previous, end),
        relative_motion(start, current),
    );
    assert_transform_close(wrapped, expected);

    let multi_wrap = track.delta(SamplingRatio::new(0.75)?, SamplingRatio::new(0.25)?, 2)?;
    assert!(start.translation.is_finite());
    assert!(end.rotation.is_finite());
    assert!(multi_wrap.translation.is_finite());
    assert!(multi_wrap.rotation.is_normalized());
    Ok(())
}

#[test]
fn pose_samples_clip_and_produces_finite_model_matrices() -> Result<(), Error> {
    let skeleton = load_skeleton(SKELETON_OZZ, SKELETON_IDENTITY)?;
    let animation = load_animation(false)?;
    let mut context = SamplingContext::new(animation.track_count())?;
    let mut pose = Pose::new(&skeleton)?;

    pose.sample(
        &skeleton,
        &animation,
        &mut context,
        SamplingRatio::new(0.5)?,
    )?;

    assert_eq!(pose.joint_count(), skeleton.joint_count());
    assert_eq!(pose.model_matrices().len(), skeleton.joint_count());
    assert!(pose.model_matrices().all(|matrix| matrix.is_finite()));
    assert!(pose.model_matrix(skeleton.joint_count()).is_none());
    Ok(())
}

#[test]
fn pose_rejects_a_different_rig_identity_with_the_same_track_count() -> Result<(), Error> {
    let skeleton = load_skeleton(SKELETON_OZZ, SKELETON_IDENTITY)?;
    let animation = load_animation_for(OTHER_SKELETON_IDENTITY, false)?;
    assert_eq!(animation.track_count(), skeleton.joint_count());
    let mut context = SamplingContext::new(animation.track_count())?;
    let mut pose = Pose::new(&skeleton)?;

    assert_eq!(
        pose.sample(
            &skeleton,
            &animation,
            &mut context,
            SamplingRatio::new(0.5)?,
        ),
        Err(Error::SkeletonIdentityMismatch),
    );
    Ok(())
}

#[test]
fn animation_set_enforces_identity_and_unique_clip_names() -> Result<(), Error> {
    let mut set = AnimationSet::new(SKELETON_IDENTITY);
    set.insert(load_animation(false)?)?;
    assert_eq!(set.len(), 1);
    assert!(set.get("Take 001").is_some());
    assert_eq!(
        set.insert(load_animation(false)?),
        Err(Error::DuplicateAnimationClip),
    );
    assert_eq!(
        set.insert(load_animation_for(OTHER_SKELETON_IDENTITY, false)?),
        Err(Error::AnimationSetSkeletonMismatch),
    );
    Ok(())
}

#[test]
fn pose_blends_normal_additive_and_partial_layers() -> Result<(), Error> {
    let skeleton = load_skeleton(SKELETON_OZZ, SKELETON_IDENTITY)?;
    let animation = load_animation(false)?;
    let mut context = SamplingContext::new(animation.track_count())?;
    let mut first = Pose::new(&skeleton)?;
    let mut second = Pose::new(&skeleton)?;
    let mut blended = Pose::new(&skeleton)?;
    first.sample(
        &skeleton,
        &animation,
        &mut context,
        SamplingRatio::new(0.25)?,
    )?;
    second.sample(
        &skeleton,
        &animation,
        &mut context,
        SamplingRatio::new(0.75)?,
    )?;

    let joint_weights = vec![1.0; skeleton.joint_count()];
    let layers = [
        BlendLayer::normal(&first, 0.25)?,
        BlendLayer::normal(&second, 0.75)?.with_joint_weights(&joint_weights),
        BlendLayer::additive(&second, 0.0)?,
    ];
    blended.blend(&skeleton, &layers, 0.1)?;

    assert!(blended.model_matrices().all(|matrix| matrix.is_finite()));
    assert!(
        blended
            .local_transforms()
            .all(|transform| transform.translation.is_finite())
    );
    Ok(())
}

#[test]
fn local_pose_can_be_modified_procedurally() -> Result<(), Error> {
    let skeleton = load_skeleton(SKELETON_OZZ, SKELETON_IDENTITY)?;
    let mut pose = Pose::new(&skeleton)?;
    let Some(mut root) = pose.local_transform(0) else {
        return Err(Error::NativeContract);
    };
    root.translation += Vec3::X;
    pose.set_local_transform(&skeleton, 0, root)?;

    let Some(updated) = pose.local_transform(0) else {
        return Err(Error::NativeContract);
    };
    assert!(updated.translation.abs_diff_eq(root.translation, 1.0e-5));
    assert!(pose.model_matrices().all(|matrix| matrix.is_finite()));
    Ok(())
}

#[test]
fn aim_and_two_bone_ik_update_safe_poses() -> Result<(), Error> {
    let skeleton = load_skeleton(OTHER_SKELETON_OZZ, OTHER_SKELETON_IDENTITY)?;
    let mut pose = Pose::new(&skeleton)?;
    let Some(root_matrix) = pose.model_matrix(0) else {
        return Err(Error::NativeContract);
    };
    let target = root_matrix.transform_point3(Vec3::X);
    let _aim = pose.apply_aim_ik(&skeleton, AimIk::new(0, target))?;

    let Some((start, middle, end)) = first_three_joint_chain(&skeleton) else {
        return Err(Error::InvalidIkChain);
    };
    let Some(end_matrix) = pose.model_matrix(end) else {
        return Err(Error::NativeContract);
    };
    let target = end_matrix.transform_point3(Vec3::ZERO);
    let _two_bone =
        pose.apply_two_bone_ik(&skeleton, TwoBoneIk::new(start, middle, end, target))?;

    assert!(pose.model_matrices().all(|matrix| matrix.is_finite()));
    Ok(())
}

#[test]
fn graph_crossfades_host_selected_states() -> Result<(), Error> {
    let idle = AnimationState::new("idle", 1.0)?;
    let mut graph = AnimationGraph::new(idle);
    let idle_id = graph.current_state();
    let run_id = graph.add_state(AnimationState::new("run", 0.5)?);
    graph.add_transition(idle_id, run_id, 0.2)?;

    graph.advance(0.25)?;
    graph.transition_to(run_id)?;
    let crossfade = graph.advance(0.1)?;
    assert!((crossfade.primary().weight() - 0.5).abs() < 1.0e-5);
    let Some(target) = crossfade.secondary() else {
        return Err(Error::NativeContract);
    };
    assert_eq!(target.state(), run_id);
    assert!((target.weight() - 0.5).abs() < 1.0e-5);

    let settled = graph.advance(0.1)?;
    assert_eq!(graph.current_state(), run_id);
    assert!(settled.secondary().is_none());
    assert_eq!(settled.primary().state(), run_id);
    Ok(())
}

#[test]
fn graph_advancement_failure_preserves_active_transition() -> Result<(), Error> {
    let idle = AnimationState::new("idle", 1.0)?.with_speed(f32::MAX)?;
    let mut graph = AnimationGraph::new(idle);
    let idle_id = graph.current_state();
    let run_id = graph.add_state(AnimationState::new("run", 1.0)?.with_speed(f32::MAX)?);
    graph.add_transition(idle_id, run_id, 1.0)?;
    graph.transition_to(run_id)?;
    let before = graph.evaluate();

    assert_eq!(graph.advance(2.0), Err(Error::InvalidGraphDelta));
    assert_eq!(graph.evaluate(), before);
    assert_eq!(graph.current_state(), idle_id);
    Ok(())
}

#[test]
fn marker_tracks_report_forward_and_wrapped_crossings() -> Result<(), Error> {
    let track = MarkerTrack::new([
        AnimationMarker::new("start", SamplingRatio::new(0.0)?),
        AnimationMarker::new("left_foot", SamplingRatio::new(0.25)?),
        AnimationMarker::new("right_foot", SamplingRatio::new(0.75)?),
        AnimationMarker::new("end", SamplingRatio::new(1.0)?),
    ])?;

    let forward = track.crossed(SamplingRatio::new(0.2)?, SamplingRatio::new(0.8)?, 0)?;
    assert_eq!(
        forward
            .iter()
            .map(|marker| marker.name())
            .collect::<Vec<_>>(),
        ["left_foot", "right_foot"],
    );

    let wrapped = track.crossed(SamplingRatio::new(0.8)?, SamplingRatio::new(0.3)?, 1)?;
    assert_eq!(
        wrapped
            .iter()
            .map(|marker| marker.name())
            .collect::<Vec<_>>(),
        ["end", "start", "left_foot"],
    );

    let complete = track.crossed(SamplingRatio::new(0.0)?, SamplingRatio::new(1.0)?, 0)?;
    assert_eq!(
        complete
            .iter()
            .map(|marker| marker.name())
            .collect::<Vec<_>>(),
        ["left_foot", "right_foot", "end"],
    );
    Ok(())
}

#[test]
fn safe_api_rejects_invalid_assets_and_sampling_inputs() -> Result<(), Error> {
    assert_eq!(
        Skeleton::from_bytes(ANIMATION_OZZ).map(|_| ()),
        Err(Error::InvalidSkeletonArchive),
    );
    assert_eq!(
        Animation::from_bytes(SKELETON_OZZ).map(|_| ()),
        Err(Error::InvalidAnimationArchive),
    );
    assert_eq!(
        SamplingContext::new(0).map(|_| ()),
        Err(Error::InvalidContextCapacity(0)),
    );
    assert_eq!(
        SamplingRatio::new(f32::NAN),
        Err(Error::InvalidSamplingRatio),
    );
    assert_eq!(SamplingRatio::new(1.01), Err(Error::InvalidSamplingRatio),);

    let skeleton = load_skeleton(SKELETON_OZZ, SKELETON_IDENTITY)?;
    let animation = load_animation(false)?;
    let mut context = SamplingContext::new(1)?;
    let mut pose = Pose::new(&skeleton)?;
    assert_eq!(
        pose.sample(
            &skeleton,
            &animation,
            &mut context,
            SamplingRatio::new(0.0)?,
        ),
        Err(Error::ContextTooSmall {
            required: animation.track_count(),
            capacity: context.max_tracks(),
        }),
    );
    assert_eq!(
        BlendLayer::normal(&pose, f32::NAN).map(|_| ()),
        Err(Error::InvalidBlendWeight),
    );
    assert_eq!(
        pose.apply_aim_ik(
            &skeleton,
            AimIk {
                weight: 2.0,
                ..AimIk::new(0, Vec3::X)
            },
        ),
        Err(Error::InvalidIkConfiguration),
    );
    Ok(())
}

#[test]
fn runtime_rejects_an_incompatible_ozz_version() -> Result<(), Error> {
    let incompatible =
        SkeletonContainer::encode(OzzVersion::new(0, 15, 0), SKELETON_IDENTITY, SKELETON_OZZ)
            .map_err(|_error| Error::InvalidSkeletonArchive)?;
    assert_eq!(
        Skeleton::from_bytes(&incompatible).map(|_| ()),
        Err(Error::UnsupportedOzzVersion),
    );
    Ok(())
}

#[test]
fn blend_rejects_invalid_partial_joint_weight_count() -> Result<(), Error> {
    let skeleton = load_skeleton(SKELETON_OZZ, SKELETON_IDENTITY)?;
    let source_pose = Pose::new(&skeleton)?;
    let mut output_pose = Pose::new(&skeleton)?;
    let joint_weights = vec![1.0; skeleton.joint_count() - 1];

    assert_eq!(
        output_pose.blend(
            &skeleton,
            &[BlendLayer::normal(&source_pose, 1.0)?.with_joint_weights(&joint_weights)],
            0.1,
        ),
        Err(Error::JointWeightCountMismatch {
            expected: skeleton.joint_count(),
            actual: joint_weights.len(),
        }),
    );
    Ok(())
}

#[test]
fn pose_remains_scoped_to_its_skeleton() -> Result<(), Error> {
    let skeleton = load_skeleton(SKELETON_OZZ, SKELETON_IDENTITY)?;
    let other_skeleton = load_skeleton(OTHER_SKELETON_OZZ, OTHER_SKELETON_IDENTITY)?;
    let mut pose = Pose::new(&skeleton)?;

    assert_eq!(
        pose.reset_to_rest(&other_skeleton),
        Err(Error::WrongSkeleton),
    );
    Ok(())
}

#[test]
fn ownership_types_have_the_intended_thread_bounds() {
    fn assert_send<T: Send>() {}
    fn assert_send_sync<T: Send + Sync>() {}

    assert_send_sync::<Skeleton>();
    assert_send_sync::<Animation>();
    assert_send::<SamplingContext>();
    assert_send::<Pose>();
}

fn first_three_joint_chain(skeleton: &Skeleton) -> Option<(usize, usize, usize)> {
    for end in 0..skeleton.joint_count() {
        let Some(middle) = skeleton.joint_parent(end).flatten() else {
            continue;
        };
        if let Some(start) = skeleton.joint_parent(middle).flatten() {
            return Some((start, middle, end));
        }
    }
    None
}

fn relative_motion(from: RootMotionTransform, to: RootMotionTransform) -> RootMotionTransform {
    let inverse_rotation = from.rotation.conjugate();
    RootMotionTransform {
        translation: inverse_rotation * (to.translation - from.translation),
        rotation: inverse_rotation * to.rotation,
    }
}

fn compose_motion(first: RootMotionTransform, second: RootMotionTransform) -> RootMotionTransform {
    RootMotionTransform {
        translation: first.translation + first.rotation * second.translation,
        rotation: first.rotation * second.rotation,
    }
}

fn assert_transform_close(actual: RootMotionTransform, expected: RootMotionTransform) {
    assert!(actual.translation.abs_diff_eq(expected.translation, 1.0e-5));
    assert!(actual.rotation.abs_diff_eq(expected.rotation, 1.0e-5));
}
