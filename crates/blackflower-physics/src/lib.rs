#![doc = include_str!("../README.md")]

mod error;
mod ffi;
mod ids;
mod types;
mod world;

pub use error::{Error, UpdateError};
pub use ids::BodyId;
pub use types::{BodySettings, MotionType, Shape, StepDelta};
pub use world::{World, WorldBuilder};

/// The Jolt Physics version compiled into this crate.
#[must_use]
pub fn jolt_version() -> (u32, u32, u32) {
    ffi::jolt_version()
}
