//! Canonical engine components.
//!
//! These are the data structures attached to entities in the ECS. Each
//! component is a plain data type with no logic — systems operate on them.
//!
//! Components are intentionally `#[repr(C)]` and `Copy` where possible to
//! make them friendly to bulk iteration and snapshot serialization.

use blackflower_math::{Quat, Vec3};
use serde::{Deserialize, Serialize};

/// Linear velocity of an entity in world space, in units per second.
#[repr(C)]
#[derive(Copy, Clone, Debug, Default, PartialEq, Serialize, Deserialize)]
pub struct Velocity(pub Vec3);

impl Velocity {
    pub const ZERO: Self = Self(Vec3::ZERO);
}
