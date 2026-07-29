use glam::Vec3;

use crate::Error;
use crate::asset::Skeleton;
use crate::ffi;

/// Configuration for rotating one joint toward a model-space target.
#[derive(Debug, Clone, Copy, PartialEq)]
pub struct AimIk {
    /// Joint to rotate.
    pub joint: usize,
    /// Target position in skeleton model space.
    pub target: Vec3,
    /// Normalized forward axis in joint-local space.
    pub forward: Vec3,
    /// Aim origin offset in joint-local space.
    pub offset: Vec3,
    /// Normalized up axis in joint-local space.
    pub up: Vec3,
    /// Non-zero orientation pole vector in model space.
    pub pole_vector: Vec3,
    /// Twist around the target direction, in radians.
    pub twist_angle: f32,
    /// Correction influence in the inclusive range `0..=1`.
    pub weight: f32,
}

impl AimIk {
    /// Construct aim IK with conventional X-forward and Y-up axes.
    #[must_use]
    pub const fn new(joint: usize, target: Vec3) -> Self {
        Self {
            joint,
            target,
            forward: Vec3::X,
            offset: Vec3::ZERO,
            up: Vec3::Y,
            pole_vector: Vec3::Y,
            twist_angle: 0.0,
            weight: 1.0,
        }
    }

    pub(crate) fn validate(self, skeleton: &Skeleton) -> Result<ffi::AimIk, Error> {
        let joint = validate_joint(skeleton, self.joint)?;
        if !self.target.is_finite()
            || !is_normalized(self.forward)
            || !self.offset.is_finite()
            || !is_normalized(self.up)
            || !is_direction(self.pole_vector)
            || !self.twist_angle.is_finite()
            || !is_unit_weight(self.weight)
        {
            return Err(Error::InvalidIkConfiguration);
        }
        Ok(ffi::AimIk {
            joint,
            target: self.target.to_array(),
            forward: self.forward.to_array(),
            offset: self.offset.to_array(),
            up: self.up.to_array(),
            pole_vector: self.pole_vector.to_array(),
            twist_angle: self.twist_angle,
            weight: self.weight,
        })
    }
}

/// Configuration for a three-joint, two-bone IK chain.
#[derive(Debug, Clone, Copy, PartialEq)]
pub struct TwoBoneIk {
    /// First joint of the chain.
    pub start_joint: usize,
    /// Bending joint of the chain.
    pub middle_joint: usize,
    /// End effector joint.
    pub end_joint: usize,
    /// Target position in skeleton model space.
    pub target: Vec3,
    /// Normalized bend axis in middle-joint local space.
    pub middle_axis: Vec3,
    /// Non-zero orientation pole vector in model space.
    pub pole_vector: Vec3,
    /// Twist around the start-to-target direction, in radians.
    pub twist_angle: f32,
    /// Fraction of the chain extension where softening begins.
    pub soften: f32,
    /// Correction influence in the inclusive range `0..=1`.
    pub weight: f32,
}

impl TwoBoneIk {
    /// Construct a two-bone chain with Z bend axis and Y pole vector.
    #[must_use]
    pub const fn new(
        start_joint: usize,
        middle_joint: usize,
        end_joint: usize,
        target: Vec3,
    ) -> Self {
        Self {
            start_joint,
            middle_joint,
            end_joint,
            target,
            middle_axis: Vec3::Z,
            pole_vector: Vec3::Y,
            twist_angle: 0.0,
            soften: 1.0,
            weight: 1.0,
        }
    }

    pub(crate) fn validate(self, skeleton: &Skeleton) -> Result<ffi::TwoBoneIk, Error> {
        let start_joint = validate_joint(skeleton, self.start_joint)?;
        let middle_joint = validate_joint(skeleton, self.middle_joint)?;
        let end_joint = validate_joint(skeleton, self.end_joint)?;
        if !skeleton.is_ancestor(self.start_joint, self.middle_joint)
            || !skeleton.is_ancestor(self.middle_joint, self.end_joint)
        {
            return Err(Error::InvalidIkChain);
        }
        if !self.target.is_finite()
            || !is_normalized(self.middle_axis)
            || !is_direction(self.pole_vector)
            || !self.twist_angle.is_finite()
            || !is_unit_weight(self.soften)
            || !is_unit_weight(self.weight)
        {
            return Err(Error::InvalidIkConfiguration);
        }
        Ok(ffi::TwoBoneIk {
            start_joint,
            middle_joint,
            end_joint,
            target: self.target.to_array(),
            middle_axis: self.middle_axis.to_array(),
            pole_vector: self.pole_vector.to_array(),
            twist_angle: self.twist_angle,
            soften: self.soften,
            weight: self.weight,
        })
    }
}

/// Result metadata produced by an inverse-kinematics job.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub struct IkOutcome {
    reached: bool,
}

impl IkOutcome {
    pub(crate) const fn new(reached: bool) -> Self {
        Self { reached }
    }

    /// Return whether the configured target was fully reached.
    #[must_use]
    pub const fn reached(self) -> bool {
        self.reached
    }
}

fn validate_joint(skeleton: &Skeleton, joint: usize) -> Result<u32, Error> {
    if !skeleton.contains_joint(joint) {
        return Err(Error::JointIndexOutOfRange {
            joint,
            joint_count: skeleton.joint_count(),
        });
    }
    u32::try_from(joint).map_err(|_error| Error::NativeContract)
}

fn is_normalized(vector: Vec3) -> bool {
    vector.is_finite() && vector.is_normalized()
}

fn is_direction(vector: Vec3) -> bool {
    vector.is_finite() && vector.length_squared() > f32::EPSILON
}

fn is_unit_weight(weight: f32) -> bool {
    weight.is_finite() && (0.0..=1.0).contains(&weight)
}
