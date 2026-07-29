use blackflower_animation::{
    AimIk, Animation, AnimationGraph, AnimationMarker, AnimationState, BlendLayer, Error,
    MarkerTrack, Pose, SamplingContext, SamplingRatio, Skeleton, TwoBoneIk, ozz_version,
    simd_implementation,
};
use glam::Vec3;

const SKELETON_BYTES: &[u8] =
    include_bytes!("../vendor/ozz-animation/media/bin/baked_skeleton.ozz");
const ANIMATION_BYTES: &[u8] =
    include_bytes!("../vendor/ozz-animation/media/bin/baked_animation.ozz");
const OTHER_SKELETON_BYTES: &[u8] =
    include_bytes!("../vendor/ozz-animation/media/bin/robot_skeleton.ozz");

#[test]
fn bindings_report_the_pinned_ozz_version_and_simd_backend() {
    assert_eq!(ozz_version(), (0, 16, 0));
    assert!(!simd_implementation().is_empty());
}

#[test]
fn runtime_archives_expose_skeleton_and_clip_metadata() -> Result<(), Error> {
    let skeleton = Skeleton::from_bytes(SKELETON_BYTES)?;
    let animation = Animation::from_bytes(ANIMATION_BYTES)?;

    assert!(skeleton.joint_count() > 1);
    assert!(skeleton.joint_name(0).is_some_and(|name| !name.is_empty()));
    assert_eq!(skeleton.joint_parent(0), Some(None));
    assert_eq!(animation.track_count(), skeleton.joint_count());
    assert!(animation.duration().is_finite());
    assert!(animation.duration() > 0.0);
    Ok(())
}

#[test]
fn pose_samples_clip_and_produces_finite_model_matrices() -> Result<(), Error> {
    let skeleton = Skeleton::from_bytes(SKELETON_BYTES)?;
    let animation = Animation::from_bytes(ANIMATION_BYTES)?;
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
fn pose_blends_normal_additive_and_partial_layers() -> Result<(), Error> {
    let skeleton = Skeleton::from_bytes(SKELETON_BYTES)?;
    let animation = Animation::from_bytes(ANIMATION_BYTES)?;
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
    let skeleton = Skeleton::from_bytes(SKELETON_BYTES)?;
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
    let skeleton = Skeleton::from_bytes(OTHER_SKELETON_BYTES)?;
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
        AnimationMarker::new("left_foot", SamplingRatio::new(0.25)?),
        AnimationMarker::new("right_foot", SamplingRatio::new(0.75)?),
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
        ["left_foot"],
    );
    Ok(())
}

#[test]
fn safe_api_rejects_invalid_assets_and_sampling_inputs() -> Result<(), Error> {
    assert_eq!(
        Skeleton::from_bytes(ANIMATION_BYTES).map(|_| ()),
        Err(Error::InvalidSkeletonArchive),
    );
    assert_eq!(
        Animation::from_bytes(SKELETON_BYTES).map(|_| ()),
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

    let skeleton = Skeleton::from_bytes(SKELETON_BYTES)?;
    let animation = Animation::from_bytes(ANIMATION_BYTES)?;
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
fn blend_rejects_invalid_partial_joint_weight_count() -> Result<(), Error> {
    let skeleton = Skeleton::from_bytes(SKELETON_BYTES)?;
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
    let skeleton = Skeleton::from_bytes(SKELETON_BYTES)?;
    let other_skeleton = Skeleton::from_bytes(OTHER_SKELETON_BYTES)?;
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
