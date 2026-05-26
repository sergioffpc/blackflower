//! Canonical engine components.
//!
//! These are the data structures attached to entities in the ECS. Each
//! component is a plain data type with no logic — systems operate on them.
//!
//! Components are intentionally `#[repr(C)]` and `Copy` where possible to
//! make them friendly to bulk iteration and snapshot serialization.

/// Spatial transform of an entity in world space.
#[derive(Copy, Clone, Debug, PartialEq)]
#[repr(C)]
pub struct Transform {
    pub translation: glam::Vec3,
    pub rotation: glam::Quat,
    pub scale: glam::Vec3,
}

impl Transform {
    pub const fn identity() -> Self {
        Self {
            translation: glam::Vec3::ZERO,
            rotation: glam::Quat::IDENTITY,
            scale: glam::Vec3::ONE,
        }
    }
}

impl Default for Transform {
    fn default() -> Self {
        Self::identity()
    }
}

/// Linear velocity of an entity in world space, in units per second.
#[derive(Copy, Clone, Debug, Default, PartialEq)]
#[repr(C)]
pub struct Velocity(pub glam::Vec3);
