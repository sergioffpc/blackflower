use glam::{Quat, Vec3};
use serde::{Deserialize, Serialize};

/// Spatial transform of an entity in world space.
#[repr(C)]
#[derive(Copy, Clone, Debug, PartialEq, Serialize, Deserialize)]
pub struct Transform {
    pub translation: Vec3,
    pub rotation: Quat,
}

impl Transform {
    pub const fn identity() -> Self {
        Self {
            translation: Vec3::ZERO,
            rotation: Quat::IDENTITY,
        }
    }

    pub fn lerp(&self, other: Self, t: f32) -> Self {
        Self {
            translation: self.translation.lerp(other.translation, t),
            rotation: self.rotation.slerp(other.rotation, t),
        }
    }
}

impl Default for Transform {
    fn default() -> Self {
        Self::identity()
    }
}
