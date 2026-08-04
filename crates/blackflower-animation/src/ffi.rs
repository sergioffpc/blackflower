#![allow(
    unsafe_code,
    unsafe_op_in_unsafe_fn,
    reason = "all raw ozz-animation calls and pointer materialization are isolated in this private module"
)]
#![allow(
    clippy::multiple_unsafe_ops_per_block,
    clippy::undocumented_unsafe_blocks,
    reason = "all unsafe operations are confined to the reviewed ozz-animation FFI boundary"
)]

use std::ffi::CStr;
use std::ptr::NonNull;

#[allow(
    dead_code,
    non_camel_case_types,
    non_snake_case,
    non_upper_case_globals,
    unsafe_code,
    reason = "generated declarations mirror the blackflower ozz-animation C wrapper"
)]
#[allow(
    clippy::all,
    clippy::cast_possible_truncation,
    clippy::cast_possible_wrap,
    clippy::ptr_offset_with_cast,
    clippy::upper_case_acronyms,
    clippy::useless_transmute,
    reason = "bindgen-generated code mirrors C layouts and is not maintained by hand"
)]
pub(crate) mod raw {
    include!(concat!(env!("OUT_DIR"), "/ozz_bindings.rs"));
}

pub(crate) type Matrix = raw::BFAnimationMatrix;
pub(crate) type Transform = raw::BFAnimationTransform;
pub(crate) type RootMotionSample = raw::BFAnimationRootMotionSample;

pub(crate) struct BlendLayer<'a> {
    pub(crate) pose: PosePtr,
    pub(crate) joint_weights: Option<&'a [f32]>,
    pub(crate) weight: f32,
    pub(crate) additive: bool,
}

pub(crate) struct AimIk {
    pub(crate) joint: u32,
    pub(crate) target: [f32; 3],
    pub(crate) forward: [f32; 3],
    pub(crate) offset: [f32; 3],
    pub(crate) up: [f32; 3],
    pub(crate) pole_vector: [f32; 3],
    pub(crate) twist_angle: f32,
    pub(crate) weight: f32,
}

pub(crate) struct TwoBoneIk {
    pub(crate) start_joint: u32,
    pub(crate) middle_joint: u32,
    pub(crate) end_joint: u32,
    pub(crate) target: [f32; 3],
    pub(crate) middle_axis: [f32; 3],
    pub(crate) pole_vector: [f32; 3],
    pub(crate) twist_angle: f32,
    pub(crate) soften: f32,
    pub(crate) weight: f32,
}

#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub(crate) enum Status {
    InvalidArgument,
    InvalidArchive,
    OutOfMemory,
    Incompatible,
    JobFailed,
    IndexOutOfRange,
    NativeFailure,
    ContractViolation,
}

#[derive(Debug, Clone, Copy)]
pub(crate) struct SkeletonPtr(NonNull<raw::BFAnimationSkeleton>);

#[derive(Debug, Clone, Copy)]
pub(crate) struct AnimationPtr(NonNull<raw::BFAnimationClip>);

#[derive(Debug, Clone, Copy)]
pub(crate) struct RootMotionPtr(NonNull<raw::BFAnimationRootMotion>);

#[derive(Debug, Clone, Copy)]
pub(crate) struct ContextPtr(NonNull<raw::BFAnimationSamplingContext>);

#[derive(Debug, Clone, Copy)]
pub(crate) struct PosePtr(NonNull<raw::BFAnimationPose>);

// SAFETY: The safe layer only exposes immutable access to loaded skeletons.
unsafe impl Send for SkeletonPtr {}
// SAFETY: The safe layer only exposes immutable access to loaded skeletons.
unsafe impl Sync for SkeletonPtr {}
// SAFETY: The safe layer only exposes immutable access to loaded clips.
unsafe impl Send for AnimationPtr {}
// SAFETY: The safe layer only exposes immutable access to loaded clips.
unsafe impl Sync for AnimationPtr {}
// SAFETY: The safe layer only exposes immutable access to loaded motion tracks.
unsafe impl Send for RootMotionPtr {}
// SAFETY: The safe layer only exposes immutable access to loaded motion tracks.
unsafe impl Sync for RootMotionPtr {}
// SAFETY: Context access is exclusively mediated by `&mut SamplingContext`.
unsafe impl Send for ContextPtr {}
// SAFETY: Pose mutation is exclusively mediated by `&mut Pose`.
unsafe impl Send for PosePtr {}

pub(crate) fn ozz_version() -> (u32, u32, u32) {
    let version = unsafe { raw::bf_animation_ozz_version() };
    (version.major, version.minor, version.patch)
}

pub(crate) fn simd_implementation() -> String {
    let pointer = unsafe { raw::bf_animation_simd_implementation() };
    if pointer.is_null() {
        return String::from("unknown");
    }
    unsafe { CStr::from_ptr(pointer) }
        .to_string_lossy()
        .into_owned()
}

pub(crate) fn load_skeleton(bytes: &[u8]) -> Result<SkeletonPtr, Status> {
    if bytes.is_empty() {
        return Err(Status::InvalidArchive);
    }
    let mut pointer = std::ptr::null_mut();
    let status =
        unsafe { raw::bf_animation_skeleton_load(bytes.as_ptr(), bytes.len(), &raw mut pointer) };
    check(status)?;
    NonNull::new(pointer)
        .map(SkeletonPtr)
        .ok_or(Status::ContractViolation)
}

pub(crate) fn destroy_skeleton(skeleton: SkeletonPtr) {
    unsafe { raw::bf_animation_skeleton_destroy(skeleton.0.as_ptr()) };
}

pub(crate) fn skeleton_joint_count(skeleton: SkeletonPtr) -> u32 {
    unsafe { raw::bf_animation_skeleton_joint_count(skeleton.0.as_ptr()) }
}

pub(crate) fn skeleton_joint_parent(skeleton: SkeletonPtr, joint: u32) -> Result<i32, Status> {
    let mut parent = -1;
    let status = unsafe {
        raw::bf_animation_skeleton_joint_parent(skeleton.0.as_ptr(), joint, &raw mut parent)
    };
    check(status)?;
    Ok(parent)
}

pub(crate) fn skeleton_joint_name(skeleton: SkeletonPtr, joint: u32) -> Result<String, Status> {
    let mut pointer = std::ptr::null();
    let mut length = 0;
    let status = unsafe {
        raw::bf_animation_skeleton_joint_name(
            skeleton.0.as_ptr(),
            joint,
            &raw mut pointer,
            &raw mut length,
        )
    };
    check(status)?;
    copy_string(pointer, length)
}

pub(crate) fn skeleton_rest_transforms(
    skeleton: SkeletonPtr,
    count: usize,
) -> Result<Box<[Transform]>, Status> {
    let mut transforms = empty_transforms(count);
    let status = unsafe {
        raw::bf_animation_skeleton_copy_rest_transforms(
            skeleton.0.as_ptr(),
            transforms.as_mut_ptr(),
            transforms.len(),
        )
    };
    check(status)?;
    Ok(transforms)
}

pub(crate) fn load_animation(bytes: &[u8]) -> Result<AnimationPtr, Status> {
    if bytes.is_empty() {
        return Err(Status::InvalidArchive);
    }
    let mut pointer = std::ptr::null_mut();
    let status =
        unsafe { raw::bf_animation_clip_load(bytes.as_ptr(), bytes.len(), &raw mut pointer) };
    check(status)?;
    NonNull::new(pointer)
        .map(AnimationPtr)
        .ok_or(Status::ContractViolation)
}

pub(crate) fn destroy_animation(animation: AnimationPtr) {
    unsafe { raw::bf_animation_clip_destroy(animation.0.as_ptr()) };
}

pub(crate) fn animation_duration(animation: AnimationPtr) -> f32 {
    unsafe { raw::bf_animation_clip_duration(animation.0.as_ptr()) }
}

pub(crate) fn animation_track_count(animation: AnimationPtr) -> u32 {
    unsafe { raw::bf_animation_clip_track_count(animation.0.as_ptr()) }
}

pub(crate) fn animation_name(animation: AnimationPtr) -> Result<String, Status> {
    let mut pointer = std::ptr::null();
    let mut length = 0;
    let status = unsafe {
        raw::bf_animation_clip_name(animation.0.as_ptr(), &raw mut pointer, &raw mut length)
    };
    check(status)?;
    copy_string(pointer, length)
}

pub(crate) fn load_root_motion(bytes: &[u8]) -> Result<RootMotionPtr, Status> {
    if bytes.is_empty() {
        return Err(Status::InvalidArchive);
    }
    let mut pointer = std::ptr::null_mut();
    let status = unsafe {
        raw::bf_animation_root_motion_load(bytes.as_ptr(), bytes.len(), &raw mut pointer)
    };
    check(status)?;
    NonNull::new(pointer)
        .map(RootMotionPtr)
        .ok_or(Status::ContractViolation)
}

pub(crate) fn destroy_root_motion(motion: RootMotionPtr) {
    unsafe { raw::bf_animation_root_motion_destroy(motion.0.as_ptr()) };
}

pub(crate) fn sample_root_motion(
    motion: RootMotionPtr,
    ratio: f32,
) -> Result<RootMotionSample, Status> {
    let mut sample = RootMotionSample::default();
    let status =
        unsafe { raw::bf_animation_root_motion_sample(motion.0.as_ptr(), ratio, &raw mut sample) };
    check(status)?;
    Ok(sample)
}

pub(crate) fn create_context(max_tracks: u32) -> Result<ContextPtr, Status> {
    let mut pointer = std::ptr::null_mut();
    let status = unsafe { raw::bf_animation_sampling_context_create(max_tracks, &raw mut pointer) };
    check(status)?;
    NonNull::new(pointer)
        .map(ContextPtr)
        .ok_or(Status::ContractViolation)
}

pub(crate) fn destroy_context(context: ContextPtr) {
    unsafe { raw::bf_animation_sampling_context_destroy(context.0.as_ptr()) };
}

pub(crate) fn context_max_tracks(context: ContextPtr) -> u32 {
    unsafe { raw::bf_animation_sampling_context_max_tracks(context.0.as_ptr()) }
}

pub(crate) fn invalidate_context(context: ContextPtr) {
    unsafe { raw::bf_animation_sampling_context_invalidate(context.0.as_ptr()) };
}

pub(crate) fn create_pose(skeleton: SkeletonPtr) -> Result<PosePtr, Status> {
    let mut pointer = std::ptr::null_mut();
    let status = unsafe { raw::bf_animation_pose_create(skeleton.0.as_ptr(), &raw mut pointer) };
    check(status)?;
    NonNull::new(pointer)
        .map(PosePtr)
        .ok_or(Status::ContractViolation)
}

pub(crate) fn destroy_pose(pose: PosePtr) {
    unsafe { raw::bf_animation_pose_destroy(pose.0.as_ptr()) };
}

pub(crate) fn pose_joint_count(pose: PosePtr) -> u32 {
    unsafe { raw::bf_animation_pose_joint_count(pose.0.as_ptr()) }
}

pub(crate) fn set_rest_pose(skeleton: SkeletonPtr, pose: PosePtr) -> Result<(), Status> {
    let status = unsafe { raw::bf_animation_pose_set_rest(skeleton.0.as_ptr(), pose.0.as_ptr()) };
    check(status)
}

pub(crate) fn sample_pose(
    skeleton: SkeletonPtr,
    animation: AnimationPtr,
    context: ContextPtr,
    ratio: f32,
    pose: PosePtr,
) -> Result<(), Status> {
    let status = unsafe {
        raw::bf_animation_pose_sample(
            skeleton.0.as_ptr(),
            animation.0.as_ptr(),
            context.0.as_ptr(),
            ratio,
            pose.0.as_ptr(),
        )
    };
    check(status)
}

pub(crate) fn blend_pose(
    skeleton: SkeletonPtr,
    layers: &[BlendLayer<'_>],
    threshold: f32,
    pose: PosePtr,
) -> Result<(), Status> {
    let native_layers = layers
        .iter()
        .map(|layer| {
            let (joint_weights, joint_weight_count) = layer
                .joint_weights
                .map_or((std::ptr::null(), 0), |weights| {
                    (weights.as_ptr(), weights.len())
                });
            raw::BFAnimationBlendLayer {
                pose: layer.pose.0.as_ptr(),
                joint_weights,
                joint_weight_count,
                weight: layer.weight,
                additive: u8::from(layer.additive),
            }
        })
        .collect::<Vec<_>>();
    let status = unsafe {
        raw::bf_animation_pose_blend(
            skeleton.0.as_ptr(),
            native_layers.as_ptr(),
            native_layers.len(),
            threshold,
            pose.0.as_ptr(),
        )
    };
    check(status)
}

pub(crate) fn empty_transforms(count: usize) -> Box<[Transform]> {
    std::iter::repeat_with(Transform::default)
        .take(count)
        .collect()
}

pub(crate) fn copy_local_transforms(
    pose: PosePtr,
    transforms: &mut [Transform],
) -> Result<(), Status> {
    let status = unsafe {
        raw::bf_animation_pose_copy_local_transforms(
            pose.0.as_ptr(),
            transforms.as_mut_ptr(),
            transforms.len(),
        )
    };
    check(status)
}

pub(crate) fn set_local_transforms(
    skeleton: SkeletonPtr,
    transforms: &[Transform],
    pose: PosePtr,
) -> Result<(), Status> {
    let status = unsafe {
        raw::bf_animation_pose_set_local_transforms(
            skeleton.0.as_ptr(),
            transforms.as_ptr(),
            transforms.len(),
            pose.0.as_ptr(),
        )
    };
    check(status)
}

pub(crate) fn set_local_transform(
    skeleton: SkeletonPtr,
    joint: u32,
    transform: &Transform,
    pose: PosePtr,
) -> Result<(), Status> {
    let status = unsafe {
        raw::bf_animation_pose_set_local_transform(
            skeleton.0.as_ptr(),
            joint,
            transform,
            pose.0.as_ptr(),
        )
    };
    check(status)
}

pub(crate) fn apply_aim_ik(
    skeleton: SkeletonPtr,
    configuration: &AimIk,
    pose: PosePtr,
) -> Result<bool, Status> {
    let native = raw::BFAnimationAimIk {
        joint: configuration.joint,
        target: configuration.target,
        forward: configuration.forward,
        offset: configuration.offset,
        up: configuration.up,
        pole_vector: configuration.pole_vector,
        twist_angle: configuration.twist_angle,
        weight: configuration.weight,
    };
    let mut reached = 0;
    let status = unsafe {
        raw::bf_animation_pose_apply_aim_ik(
            skeleton.0.as_ptr(),
            &raw const native,
            pose.0.as_ptr(),
            &raw mut reached,
        )
    };
    check(status)?;
    Ok(reached != 0)
}

pub(crate) fn apply_two_bone_ik(
    skeleton: SkeletonPtr,
    configuration: &TwoBoneIk,
    pose: PosePtr,
) -> Result<bool, Status> {
    let native = raw::BFAnimationTwoBoneIk {
        start_joint: configuration.start_joint,
        middle_joint: configuration.middle_joint,
        end_joint: configuration.end_joint,
        target: configuration.target,
        middle_axis: configuration.middle_axis,
        pole_vector: configuration.pole_vector,
        twist_angle: configuration.twist_angle,
        soften: configuration.soften,
        weight: configuration.weight,
    };
    let mut reached = 0;
    let status = unsafe {
        raw::bf_animation_pose_apply_two_bone_ik(
            skeleton.0.as_ptr(),
            &raw const native,
            pose.0.as_ptr(),
            &raw mut reached,
        )
    };
    check(status)?;
    Ok(reached != 0)
}

pub(crate) fn empty_matrices(count: usize) -> Box<[Matrix]> {
    std::iter::repeat_with(Matrix::default)
        .take(count)
        .collect()
}

pub(crate) fn copy_model_matrices(pose: PosePtr, matrices: &mut [Matrix]) -> Result<(), Status> {
    let status = unsafe {
        raw::bf_animation_pose_copy_model_matrices(
            pose.0.as_ptr(),
            matrices.as_mut_ptr(),
            matrices.len(),
        )
    };
    check(status)
}

pub(crate) fn matrix_columns(matrix: &Matrix) -> &[f32; 16] {
    &matrix.columns
}

fn copy_string(pointer: *const std::ffi::c_char, length: usize) -> Result<String, Status> {
    if pointer.is_null() {
        return Err(Status::ContractViolation);
    }
    let bytes = unsafe { std::slice::from_raw_parts(pointer.cast::<u8>(), length) };
    Ok(String::from_utf8_lossy(bytes).into_owned())
}

fn check(status: i32) -> Result<(), Status> {
    let Ok(status) = u32::try_from(status) else {
        return Err(Status::ContractViolation);
    };
    match status {
        raw::BF_ANIMATION_STATUS_OK => Ok(()),
        raw::BF_ANIMATION_STATUS_INVALID_ARGUMENT => Err(Status::InvalidArgument),
        raw::BF_ANIMATION_STATUS_INVALID_ARCHIVE => Err(Status::InvalidArchive),
        raw::BF_ANIMATION_STATUS_OUT_OF_MEMORY => Err(Status::OutOfMemory),
        raw::BF_ANIMATION_STATUS_INCOMPATIBLE => Err(Status::Incompatible),
        raw::BF_ANIMATION_STATUS_JOB_FAILED => Err(Status::JobFailed),
        raw::BF_ANIMATION_STATUS_INDEX_OUT_OF_RANGE => Err(Status::IndexOutOfRange),
        raw::BF_ANIMATION_STATUS_NATIVE_FAILURE => Err(Status::NativeFailure),
        _ => Err(Status::ContractViolation),
    }
}
