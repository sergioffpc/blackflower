use glam::{Quat, Vec3};
use serde::{Deserialize, Serialize};

/// Spatial transform of an entity in world space.
#[repr(C)]
#[derive(Copy, Clone, Debug, PartialEq, Serialize, Deserialize)]
pub struct Transform {
    pub translation: Vec3,
    pub rotation: Quat,
    pub scale: Vec3,
}

impl Transform {
    pub const fn identity() -> Self {
        Self {
            translation: Vec3::ZERO,
            rotation: Quat::IDENTITY,
            scale: Vec3::ONE,
        }
    }
}

impl Default for Transform {
    fn default() -> Self {
        Self::identity()
    }
}
