use glam::{Quat, Vec3};

use crate::Error;

/// A validated normalized time used to sample an animation clip.
#[derive(Debug, Clone, Copy, PartialEq)]
pub struct SamplingRatio(f32);

impl SamplingRatio {
    /// Construct a finite ratio in the inclusive range `0..=1`.
    pub fn new(ratio: f32) -> Result<Self, Error> {
        if ratio.is_finite() && (0.0..=1.0).contains(&ratio) {
            Ok(Self(ratio))
        } else {
            Err(Error::InvalidSamplingRatio)
        }
    }

    /// Return the normalized sampling time.
    #[must_use]
    pub const fn get(self) -> f32 {
        self.0
    }

    pub(crate) const fn from_validated(ratio: f32) -> Self {
        Self(ratio)
    }
}

/// One joint's translation, rotation, and scale in skeleton-local space.
#[derive(Debug, Clone, Copy, PartialEq)]
pub struct JointTransform {
    /// Joint translation.
    pub translation: Vec3,
    /// Normalized joint rotation.
    pub rotation: Quat,
    /// Joint scale.
    pub scale: Vec3,
}

impl JointTransform {
    /// Identity local transform.
    pub const IDENTITY: Self = Self {
        translation: Vec3::ZERO,
        rotation: Quat::IDENTITY,
        scale: Vec3::ONE,
    };

    /// Construct a local joint transform.
    #[must_use]
    pub const fn new(translation: Vec3, rotation: Quat, scale: Vec3) -> Self {
        Self {
            translation,
            rotation,
            scale,
        }
    }

    pub(crate) fn is_valid(self) -> bool {
        self.translation.is_finite()
            && self.rotation.is_finite()
            && self.rotation.is_normalized()
            && self.scale.is_finite()
    }
}

impl Default for JointTransform {
    fn default() -> Self {
        Self::IDENTITY
    }
}
