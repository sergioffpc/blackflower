use glam::{Quat, Vec3};

use crate::error::map_native_failure;
use crate::{Error, SamplingRatio, ffi};

/// Rigid transform sampled from an extracted root-motion track.
#[derive(Debug, Clone, Copy, PartialEq)]
pub struct RootMotionTransform {
    /// Translation relative to the authored extraction reference.
    pub translation: Vec3,
    /// Rotation relative to the authored extraction reference.
    pub rotation: Quat,
}

impl RootMotionTransform {
    /// Identity motion.
    pub const IDENTITY: Self = Self {
        translation: Vec3::ZERO,
        rotation: Quat::IDENTITY,
    };

    fn inverse(self) -> Self {
        let rotation = self.rotation.conjugate();
        Self {
            translation: rotation * -self.translation,
            rotation,
        }
    }

    fn then(self, next: Self) -> Self {
        Self {
            translation: self.translation + self.rotation * next.translation,
            rotation: self.rotation * next.rotation,
        }
    }
}

/// Immutable translation and rotation tracks extracted by ozz.
#[derive(Debug)]
pub struct RootMotionTrack {
    pointer: ffi::RootMotionPtr,
}

impl RootMotionTrack {
    pub(crate) fn from_ozz_bytes(bytes: &[u8]) -> Result<Self, Error> {
        let pointer = ffi::load_root_motion(bytes).map_err(|status| match status {
            ffi::Status::InvalidArgument | ffi::Status::InvalidArchive => {
                Error::InvalidRootMotionArchive
            }
            ffi::Status::OutOfMemory
            | ffi::Status::Incompatible
            | ffi::Status::JobFailed
            | ffi::Status::IndexOutOfRange
            | ffi::Status::NativeFailure
            | ffi::Status::ContractViolation => map_native_failure(status),
        })?;
        Ok(Self { pointer })
    }

    /// Sample the absolute extracted transform at a normalized time.
    pub fn sample(&self, ratio: SamplingRatio) -> Result<RootMotionTransform, Error> {
        let sample =
            ffi::sample_root_motion(self.pointer, ratio.get()).map_err(map_native_failure)?;
        let transform = RootMotionTransform {
            translation: Vec3::from_array(sample.translation),
            rotation: Quat::from_array(sample.rotation),
        };
        if !transform.translation.is_finite()
            || !transform.rotation.is_finite()
            || !transform.rotation.is_normalized()
        {
            return Err(Error::NativeContract);
        }
        Ok(transform)
    }

    /// Calculate motion crossed between two ratios and any completed loops.
    ///
    /// `wraps` is the number of transitions from ratio one back to zero.
    pub fn delta(
        &self,
        previous: SamplingRatio,
        current: SamplingRatio,
        wraps: u32,
    ) -> Result<RootMotionTransform, Error> {
        if wraps == 0 {
            if current.get() < previous.get() {
                return Err(Error::InvalidRootMotionTraversal);
            }
            return Ok(relative(self.sample(previous)?, self.sample(current)?));
        }

        let start = self.sample(SamplingRatio::from_validated(0.0))?;
        let end = self.sample(SamplingRatio::from_validated(1.0))?;
        let previous_transform = self.sample(previous)?;
        let current_transform = self.sample(current)?;
        let mut total = relative(previous_transform, end);
        if wraps > 1 {
            total = total.then(power(relative(start, end), wraps - 1));
        }
        Ok(total.then(relative(start, current_transform)))
    }
}

impl Drop for RootMotionTrack {
    fn drop(&mut self) {
        ffi::destroy_root_motion(self.pointer);
    }
}

fn relative(from: RootMotionTransform, to: RootMotionTransform) -> RootMotionTransform {
    from.inverse().then(to)
}

fn power(mut value: RootMotionTransform, mut exponent: u32) -> RootMotionTransform {
    let mut result = RootMotionTransform::IDENTITY;
    while exponent > 0 {
        if exponent & 1 != 0 {
            result = result.then(value);
        }
        value = value.then(value);
        exponent >>= 1;
    }
    result
}
