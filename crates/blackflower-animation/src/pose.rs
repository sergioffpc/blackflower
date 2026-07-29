use std::sync::{Arc, Weak};

use glam::Mat4;

use crate::asset::{Animation, Skeleton};
use crate::context::SamplingContext;
use crate::error::map_native_failure;
use crate::ffi::{self, Status};
use crate::{Error, SamplingRatio};

/// Local sampled transforms and cached model-space joint matrices.
#[derive(Debug)]
pub struct Pose {
    pointer: ffi::PosePtr,
    skeleton: Weak<()>,
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
            matrices: ffi::empty_matrices(joint_count),
        };
        pose.refresh_matrices()?;
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
        self.refresh_matrices()
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
        self.refresh_matrices()
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

    fn refresh_matrices(&mut self) -> Result<(), Error> {
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
