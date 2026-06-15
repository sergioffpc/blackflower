use blackflower_math::Vec3;
use serde::{Deserialize, Serialize};

#[repr(C)]
#[derive(Copy, Clone, Debug, Default, PartialEq, Serialize, Deserialize)]
pub struct Velocity(pub Vec3);

impl Velocity {
    pub const ZERO: Self = Self(Vec3::ZERO);
}
