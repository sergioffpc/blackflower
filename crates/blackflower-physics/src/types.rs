use crate::{Error, Shape};
use glam::{Quat, Vec3A};

/// Jolt rigid-body motion mode.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum MotionType {
    /// Body cannot move.
    Static,
    /// Body moves through explicitly supplied velocities.
    Kinematic,
    /// Body responds to forces and contacts.
    Dynamic,
}

impl MotionType {
    pub(crate) const fn raw(self) -> u32 {
        match self {
            Self::Static => crate::ffi::raw::BF_PHYSICS_MOTION_STATIC,
            Self::Kinematic => crate::ffi::raw::BF_PHYSICS_MOTION_KINEMATIC,
            Self::Dynamic => crate::ffi::raw::BF_PHYSICS_MOTION_DYNAMIC,
        }
    }
}

/// Validated settings used to create a rigid body.
#[derive(Debug, Clone, PartialEq)]
pub struct BodySettings {
    pub(crate) shape: Shape,
    pub(crate) motion_type: MotionType,
    pub(crate) position: Vec3A,
    pub(crate) rotation: Quat,
    pub(crate) active: bool,
}

impl BodySettings {
    /// Construct body settings at the origin with identity rotation.
    #[must_use]
    pub const fn new(shape: Shape, motion_type: MotionType) -> Self {
        Self {
            shape,
            motion_type,
            position: Vec3A::ZERO,
            rotation: Quat::IDENTITY,
            active: !matches!(motion_type, MotionType::Static),
        }
    }

    /// Set a finite world-space position.
    pub fn with_position(mut self, position: Vec3A) -> Result<Self, Error> {
        self.position = validate_vector(position)?;
        Ok(self)
    }

    /// Set a finite, normalized world-space rotation.
    pub fn with_rotation(mut self, rotation: Quat) -> Result<Self, Error> {
        self.rotation = validate_rotation(rotation)?;
        Ok(self)
    }

    /// Select whether the body starts active.
    #[must_use]
    pub const fn with_active(mut self, active: bool) -> Self {
        self.active = active;
        self
    }
}

pub(crate) fn validate_vector(value: Vec3A) -> Result<Vec3A, Error> {
    if value.is_finite() {
        Ok(value)
    } else {
        Err(Error::InvalidVector)
    }
}

pub(crate) fn validate_rotation(value: Quat) -> Result<Quat, Error> {
    if value.is_finite() && value.is_normalized() {
        Ok(value)
    } else {
        Err(Error::InvalidRotation)
    }
}
