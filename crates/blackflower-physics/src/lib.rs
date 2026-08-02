#![doc = include_str!("../README.md")]

mod character;
mod contact;
mod error;
mod ffi;
mod ids;
mod raycast;
mod shape;
mod types;
mod world;

pub use character::{CharacterGround, CharacterSettings, CharacterState, GroundState};
pub use contact::{ContactEvent, ContactEventKind, ContactManifold, ContactPoint};
pub use error::{Error, UpdateError};
pub use ids::{BodyId, CharacterId, SubShapeId};
pub use raycast::RayHit;
pub use shape::{CompoundShapeChild, MAX_CONVEX_HULL_POINTS, Shape};
pub use types::{BodySettings, MotionType, StepDelta};
pub use world::{World, WorldBuilder};

/// The Jolt Physics version compiled into this crate.
#[must_use]
pub fn jolt_version() -> (u32, u32, u32) {
    ffi::jolt_version()
}
