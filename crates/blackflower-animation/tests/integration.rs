use blackflower_animation::{
    Animation, Error, Pose, SamplingContext, SamplingRatio, Skeleton, ozz_version,
    simd_implementation,
};

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
