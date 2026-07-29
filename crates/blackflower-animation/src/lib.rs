#![doc = include_str!("../README.md")]

mod asset;
mod context;
mod error;
mod ffi;
mod pose;
mod types;

pub use asset::{Animation, Skeleton};
pub use context::SamplingContext;
pub use error::Error;
pub use glam::Mat4;
pub use pose::Pose;
pub use types::SamplingRatio;

/// The ozz-animation version compiled into this crate.
#[must_use]
pub fn ozz_version() -> (u32, u32, u32) {
    ffi::ozz_version()
}

/// The SIMD implementation selected by the native ozz-animation build.
#[must_use]
pub fn simd_implementation() -> String {
    ffi::simd_implementation()
}
