use glam::{Quat, Vec3A};

use crate::error::Error;
use crate::ffi;
use crate::ids::{BodyId, SubShapeId, WorldKey};
use crate::types::{Shape, ShapeKind, validate_rotation, validate_vector};

const DEFAULT_CHARACTER_MASS: f32 = 80.0;
const DEFAULT_CHARACTER_FRICTION: f32 = 0.2;
const DEFAULT_CHARACTER_GRAVITY_FACTOR: f32 = 1.0;
const DEFAULT_MAX_SLOPE_ANGLE_RADIANS: f32 = 0.872_664_63;

/// Validated settings for a rigid-body capsule character controller.
///
/// The controller position is at the capsule's bottom rather than its center.
#[derive(Debug, Clone, Copy, PartialEq)]
pub struct CharacterSettings {
    pub(crate) position: Vec3A,
    pub(crate) rotation: Quat,
    pub(crate) capsule_half_height: f32,
    pub(crate) capsule_radius: f32,
    pub(crate) mass: f32,
    pub(crate) friction: f32,
    pub(crate) gravity_factor: f32,
    pub(crate) max_slope_angle_radians: f32,
    pub(crate) active: bool,
}

impl CharacterSettings {
    /// Construct a controller using a validated capsule shape.
    pub fn new(capsule: Shape) -> Result<Self, Error> {
        let ShapeKind::Capsule {
            half_height,
            radius,
        } = capsule.kind()
        else {
            return Err(Error::InvalidCharacterShape);
        };
        Ok(Self {
            position: Vec3A::ZERO,
            rotation: Quat::IDENTITY,
            capsule_half_height: half_height,
            capsule_radius: radius,
            mass: DEFAULT_CHARACTER_MASS,
            friction: DEFAULT_CHARACTER_FRICTION,
            gravity_factor: DEFAULT_CHARACTER_GRAVITY_FACTOR,
            max_slope_angle_radians: DEFAULT_MAX_SLOPE_ANGLE_RADIANS,
            active: true,
        })
    }

    /// Set the initial finite world-space position.
    pub fn with_position(mut self, position: Vec3A) -> Result<Self, Error> {
        self.position = validate_vector(position)?;
        Ok(self)
    }

    /// Set the initial finite, normalized world-space rotation.
    pub fn with_rotation(mut self, rotation: Quat) -> Result<Self, Error> {
        self.rotation = validate_rotation(rotation)?;
        Ok(self)
    }

    /// Set the finite, strictly positive character mass in kilograms.
    pub fn with_mass(mut self, mass: f32) -> Result<Self, Error> {
        if !mass.is_finite() || mass <= 0.0 {
            return Err(Error::InvalidCharacterMass);
        }
        self.mass = mass;
        Ok(self)
    }

    /// Set the finite, non-negative character friction.
    pub fn with_friction(mut self, friction: f32) -> Result<Self, Error> {
        if !friction.is_finite() || friction < 0.0 {
            return Err(Error::InvalidCharacterFriction);
        }
        self.friction = friction;
        Ok(self)
    }

    /// Set the finite multiplier applied to world gravity.
    pub fn with_gravity_factor(mut self, gravity_factor: f32) -> Result<Self, Error> {
        if !gravity_factor.is_finite() {
            return Err(Error::InvalidCharacterGravityFactor);
        }
        self.gravity_factor = gravity_factor;
        Ok(self)
    }

    /// Set the maximum walkable slope angle in radians.
    pub fn with_max_slope_angle(mut self, radians: f32) -> Result<Self, Error> {
        if !radians.is_finite() || !(0.0..=std::f32::consts::FRAC_PI_2).contains(&radians) {
            return Err(Error::InvalidCharacterSlopeAngle);
        }
        self.max_slope_angle_radians = radians;
        Ok(self)
    }

    /// Select whether the controller starts active.
    #[must_use]
    pub const fn with_active(mut self, active: bool) -> Self {
        self.active = active;
        self
    }
}

/// Support state derived for a character after the rigid-body step.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum GroundState {
    /// The character is supported by walkable ground.
    OnGround,
    /// The character is supported by a slope that is too steep to climb.
    OnSteepGround,
    /// The character touches something that does not support it.
    NotSupported,
    /// The character has no supporting contact.
    InAir,
}

/// Ground facts captured by a character controller.
#[derive(Debug, Clone, Copy, PartialEq)]
pub struct CharacterGround {
    /// Current support classification.
    pub state: GroundState,
    /// Supporting or touched body, when one exists.
    pub body: Option<BodyId>,
    /// Child shape of the supporting or touched body.
    pub sub_shape: Option<SubShapeId>,
    /// World-space ground contact position.
    pub position: Vec3A,
    /// World-space ground contact normal.
    pub normal: Vec3A,
    /// World-space velocity of the ground at the contact point.
    pub velocity: Vec3A,
}

/// State captured from a rigid-body character controller.
#[derive(Debug, Clone, Copy, PartialEq)]
pub struct CharacterState {
    /// Rigid body driven by the controller.
    pub body: BodyId,
    /// World-space character position.
    pub position: Vec3A,
    /// World-space character rotation.
    pub rotation: Quat,
    /// World-space linear velocity.
    pub linear_velocity: Vec3A,
    /// Ground facts refreshed after the latest physics step.
    pub ground: CharacterGround,
}

pub(crate) fn state_from_raw(
    value: ffi::raw::BFPhysicsCharacterState,
    world: WorldKey,
) -> Result<CharacterState, Error> {
    let state = ground_state(value.ground_state)?;
    let ground_exists = value.ground_body_id != u32::MAX;
    Ok(CharacterState {
        body: BodyId {
            raw: value.body_id,
            world,
        },
        position: ffi::safe_vec(value.position),
        rotation: ffi::safe_quat(value.rotation),
        linear_velocity: ffi::safe_vec(value.linear_velocity),
        ground: CharacterGround {
            state,
            body: ground_exists.then_some(BodyId {
                raw: value.ground_body_id,
                world,
            }),
            sub_shape: ground_exists.then_some(SubShapeId(value.ground_sub_shape_id)),
            position: ffi::safe_vec(value.ground_position),
            normal: ffi::safe_vec(value.ground_normal),
            velocity: ffi::safe_vec(value.ground_velocity),
        },
    })
}

fn ground_state(value: u32) -> Result<GroundState, Error> {
    match value {
        ffi::raw::BF_PHYSICS_GROUND_ON_GROUND => Ok(GroundState::OnGround),
        ffi::raw::BF_PHYSICS_GROUND_ON_STEEP_GROUND => Ok(GroundState::OnSteepGround),
        ffi::raw::BF_PHYSICS_GROUND_NOT_SUPPORTED => Ok(GroundState::NotSupported),
        ffi::raw::BF_PHYSICS_GROUND_IN_AIR => Ok(GroundState::InAir),
        _ => Err(Error::NativeContract),
    }
}
