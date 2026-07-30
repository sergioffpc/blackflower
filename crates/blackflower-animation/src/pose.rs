use std::sync::{Arc, Weak};

use glam::{Mat4, Quat, Vec3};

use crate::asset::{Animation, Skeleton};
use crate::blend::{BlendLayer, BlendMode};
use crate::context::SamplingContext;
use crate::error::map_native_failure;
use crate::ffi::{self, Status};
use crate::ik::{AimIk, IkOutcome, TwoBoneIk};
use crate::{Error, JointTransform, SamplingRatio};

/// Local sampled transforms and cached model-space joint matrices.
#[derive(Debug)]
pub struct Pose {
    pointer: ffi::PosePtr,
    skeleton: Weak<()>,
    transforms: Box<[ffi::Transform]>,
    matrices: Box<[ffi::Matrix]>,
}

impl Pose {
    /// Allocate a pose initialized from the skeleton rest pose.
    pub fn new(skeleton: &Skeleton) -> Result<Self, Error> {
        let pointer = ffi::create_pose(skeleton.pointer).map_err(map_native)?;
        let joint_count = usize::try_from(ffi::pose_joint_count(pointer))
            .map_err(|_error| Error::NativeContract)?;
        let mut pose = Self {
            pointer,
            skeleton: Arc::downgrade(&skeleton.identity),
            transforms: ffi::empty_transforms(joint_count),
            matrices: ffi::empty_matrices(joint_count),
        };
        pose.refresh()?;
        Ok(pose)
    }

    /// Return the number of model-space joint matrices.
    #[must_use]
    pub fn joint_count(&self) -> usize {
        self.matrices.len()
    }

    /// Reset this pose to its skeleton's rest pose.
    pub fn reset_to_rest(&mut self, skeleton: &Skeleton) -> Result<(), Error> {
        self.validate_skeleton(skeleton)?;
        ffi::set_rest_pose(skeleton.pointer, self.pointer).map_err(map_native)?;
        self.refresh()
    }

    /// Sample a clip and update all model-space joint matrices.
    pub fn sample(
        &mut self,
        skeleton: &Skeleton,
        animation: &Animation,
        context: &mut SamplingContext,
        ratio: SamplingRatio,
    ) -> Result<(), Error> {
        self.validate_skeleton(skeleton)?;
        if animation.skeleton_identity() != skeleton.skeleton_identity() {
            return Err(Error::SkeletonIdentityMismatch);
        }
        if animation.track_count() != skeleton.joint_count() {
            return Err(Error::TrackCountMismatch {
                joints: skeleton.joint_count(),
                tracks: animation.track_count(),
            });
        }
        if context.max_tracks() < animation.track_count() {
            return Err(Error::ContextTooSmall {
                required: animation.track_count(),
                capacity: context.max_tracks(),
            });
        }

        context.prepare(animation);
        ffi::sample_pose(
            skeleton.pointer,
            animation.pointer,
            context.pointer,
            ratio.get(),
            self.pointer,
        )
        .map_err(map_native)?;
        self.refresh()
    }

    /// Blend normal, additive, and optionally per-joint weighted input poses.
    ///
    /// An empty layer list produces the skeleton rest pose. Input poses and the
    /// output pose must all belong to `skeleton`.
    pub fn blend(
        &mut self,
        skeleton: &Skeleton,
        layers: &[BlendLayer<'_>],
        threshold: f32,
    ) -> Result<(), Error> {
        self.validate_skeleton(skeleton)?;
        validate_blend_layers(skeleton, layers, threshold)?;
        let native_layers = layers
            .iter()
            .map(|layer| ffi::BlendLayer {
                pose: layer.pose.pointer,
                joint_weights: layer.joint_weights,
                weight: layer.weight,
                additive: layer.mode == BlendMode::Additive,
            })
            .collect::<Vec<_>>();
        ffi::blend_pose(skeleton.pointer, &native_layers, threshold, self.pointer)
            .map_err(map_native)?;
        self.refresh()
    }

    /// Return one local joint transform, or `None` when out of range.
    #[must_use]
    pub fn local_transform(&self, joint: usize) -> Option<JointTransform> {
        self.transforms.get(joint).map(transform_from_native)
    }

    /// Iterate over local transforms in skeleton joint order.
    pub fn local_transforms(&self) -> impl ExactSizeIterator<Item = JointTransform> + '_ {
        self.transforms.iter().map(transform_from_native)
    }

    /// Replace the complete local pose and rebuild model-space matrices.
    pub fn set_local_transforms(
        &mut self,
        skeleton: &Skeleton,
        transforms: &[JointTransform],
    ) -> Result<(), Error> {
        self.validate_skeleton(skeleton)?;
        validate_local_transforms(skeleton, transforms)?;
        let native = transforms
            .iter()
            .copied()
            .map(transform_to_native)
            .collect::<Vec<_>>();
        ffi::set_local_transforms(skeleton.pointer, &native, self.pointer).map_err(map_native)?;
        self.refresh()
    }

    /// Replace one local joint transform and rebuild model-space matrices.
    pub fn set_local_transform(
        &mut self,
        skeleton: &Skeleton,
        joint: usize,
        transform: JointTransform,
    ) -> Result<(), Error> {
        self.validate_skeleton(skeleton)?;

        let mut transforms = self.local_transforms().collect::<Vec<_>>();
        let joint_count = transforms.len();
        let Some(slot) = transforms.get_mut(joint) else {
            return Err(Error::JointIndexOutOfRange { joint, joint_count });
        };
        *slot = transform;
        self.set_local_transforms(skeleton, &transforms)
    }

    /// Apply aim inverse kinematics and rebuild the final pose.
    pub fn apply_aim_ik(
        &mut self,
        skeleton: &Skeleton,
        configuration: AimIk,
    ) -> Result<IkOutcome, Error> {
        self.validate_skeleton(skeleton)?;
        let configuration = configuration.validate(skeleton)?;
        let reached = ffi::apply_aim_ik(skeleton.pointer, &configuration, self.pointer)
            .map_err(map_native)?;
        self.refresh()?;
        Ok(IkOutcome::new(reached))
    }

    /// Apply two-bone inverse kinematics and rebuild the final pose.
    pub fn apply_two_bone_ik(
        &mut self,
        skeleton: &Skeleton,
        configuration: TwoBoneIk,
    ) -> Result<IkOutcome, Error> {
        self.validate_skeleton(skeleton)?;
        let configuration = configuration.validate(skeleton)?;
        let reached = ffi::apply_two_bone_ik(skeleton.pointer, &configuration, self.pointer)
            .map_err(map_native)?;
        self.refresh()?;
        Ok(IkOutcome::new(reached))
    }

    /// Return one model-space matrix, or `None` when the joint is out of range.
    #[must_use]
    pub fn model_matrix(&self, joint: usize) -> Option<Mat4> {
        self.matrices
            .get(joint)
            .map(|matrix| Mat4::from_cols_array(ffi::matrix_columns(matrix)))
    }

    /// Iterate over all model-space matrices in skeleton joint order.
    pub fn model_matrices(&self) -> impl ExactSizeIterator<Item = Mat4> + '_ {
        self.matrices
            .iter()
            .map(|matrix| Mat4::from_cols_array(ffi::matrix_columns(matrix)))
    }

    fn validate_skeleton(&self, skeleton: &Skeleton) -> Result<(), Error> {
        let identity = Arc::downgrade(&skeleton.identity);
        if Weak::ptr_eq(&self.skeleton, &identity) {
            Ok(())
        } else {
            Err(Error::WrongSkeleton)
        }
    }

    fn refresh(&mut self) -> Result<(), Error> {
        ffi::copy_local_transforms(self.pointer, &mut self.transforms).map_err(map_native)?;
        ffi::copy_model_matrices(self.pointer, &mut self.matrices).map_err(map_native)
    }
}

impl Drop for Pose {
    fn drop(&mut self) {
        ffi::destroy_pose(self.pointer);
    }
}

const fn map_native(status: Status) -> Error {
    match status {
        Status::JobFailed => Error::NativeJobFailed,
        Status::InvalidArgument
        | Status::InvalidArchive
        | Status::OutOfMemory
        | Status::Incompatible
        | Status::IndexOutOfRange
        | Status::NativeFailure
        | Status::ContractViolation => map_native_failure(status),
    }
}

fn validate_blend_layers(
    skeleton: &Skeleton,
    layers: &[BlendLayer<'_>],
    threshold: f32,
) -> Result<(), Error> {
    if !threshold.is_finite() || threshold <= 0.0 {
        return Err(Error::InvalidBlendThreshold);
    }
    for layer in layers {
        layer.pose.validate_skeleton(skeleton)?;
        if let Some(weights) = layer.joint_weights {
            validate_joint_weights(skeleton, weights)?;
        }
    }
    Ok(())
}

fn validate_joint_weights(skeleton: &Skeleton, weights: &[f32]) -> Result<(), Error> {
    if weights.len() != skeleton.joint_count() {
        return Err(Error::JointWeightCountMismatch {
            expected: skeleton.joint_count(),
            actual: weights.len(),
        });
    }
    if let Some(joint) = weights
        .iter()
        .position(|weight| !weight.is_finite() || !(0.0..=1.0).contains(weight))
    {
        return Err(Error::InvalidJointWeight { joint });
    }
    Ok(())
}

fn validate_local_transforms(
    skeleton: &Skeleton,
    transforms: &[JointTransform],
) -> Result<(), Error> {
    if transforms.len() != skeleton.joint_count() {
        return Err(Error::LocalTransformCountMismatch {
            expected: skeleton.joint_count(),
            actual: transforms.len(),
        });
    }
    if let Some(joint) = transforms
        .iter()
        .position(|transform| !transform.is_valid())
    {
        return Err(Error::InvalidJointTransform { joint });
    }
    Ok(())
}

fn transform_from_native(transform: &ffi::Transform) -> JointTransform {
    JointTransform::new(
        Vec3::from_array(transform.translation),
        Quat::from_array(transform.rotation),
        Vec3::from_array(transform.scale),
    )
}

fn transform_to_native(transform: JointTransform) -> ffi::Transform {
    ffi::Transform {
        translation: transform.translation.to_array(),
        rotation: transform.rotation.to_array(),
        scale: transform.scale.to_array(),
    }
}
