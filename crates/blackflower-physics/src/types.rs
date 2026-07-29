use crate::Error;
use glam::{Quat, Vec3A};

/// Collision shape supported by the initial safe binding surface.
#[derive(Debug, Clone, Copy, PartialEq)]
pub struct Shape(ShapeKind);

#[derive(Debug, Clone, Copy, PartialEq)]
pub(crate) enum ShapeKind {
    Sphere { radius: f32 },
    Box { half_extent: Vec3A },
    Capsule { half_height: f32, radius: f32 },
}

impl Shape {
    /// Construct a sphere with a finite, positive radius.
    pub fn sphere(radius: f32) -> Result<Self, Error> {
        if radius.is_finite() && radius > 0.0 {
            Ok(Self(ShapeKind::Sphere { radius }))
        } else {
            Err(Error::InvalidShape)
        }
    }

    /// Construct a box with finite, positive half extents.
    pub fn cuboid(half_extent: Vec3A) -> Result<Self, Error> {
        validate_vector(half_extent).map_err(|_error| Error::InvalidShape)?;
        if half_extent.x > 0.0 && half_extent.y > 0.0 && half_extent.z > 0.0 {
            Ok(Self(ShapeKind::Box { half_extent }))
        } else {
            Err(Error::InvalidShape)
        }
    }

    /// Construct a Y-axis capsule from its cylinder half-height and radius.
    pub fn capsule(half_height: f32, radius: f32) -> Result<Self, Error> {
        if half_height.is_finite() && half_height > 0.0 && radius.is_finite() && radius > 0.0 {
            Ok(Self(ShapeKind::Capsule {
                half_height,
                radius,
            }))
        } else {
            Err(Error::InvalidShape)
        }
    }

    pub(crate) const fn kind(self) -> ShapeKind {
        self.0
    }
}

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
#[derive(Debug, Clone, Copy, PartialEq)]
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

/// Validated simulation-step duration in seconds.
#[derive(Debug, Clone, Copy, PartialEq)]
pub struct StepDelta(f32);

impl StepDelta {
    /// Construct a finite, strictly positive step duration.
    pub fn from_seconds(seconds: f32) -> Result<Self, Error> {
        if seconds.is_finite() && seconds > 0.0 {
            Ok(Self(seconds))
        } else {
            Err(Error::InvalidStepDelta)
        }
    }

    /// Return the duration in seconds.
    #[must_use]
    pub const fn as_seconds(self) -> f32 {
        self.0
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
