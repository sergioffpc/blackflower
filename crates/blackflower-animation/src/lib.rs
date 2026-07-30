#![doc = include_str!("../README.md")]

mod asset;
mod blend;
mod clip;
mod context;
#[cfg(feature = "cooking")]
pub mod cooking;
mod error;
mod ffi;
mod graph;
mod ik;
mod marker;
mod motion;
mod pose;
mod types;

pub use asset::{Animation, Skeleton};
pub use blackflower_animation_format::SkeletonIdentity;
pub use blend::{BlendLayer, BlendMode};
pub use clip::{AnimationClipDescriptor, AnimationSet};
pub use context::SamplingContext;
pub use error::Error;
pub use glam::Mat4;
pub use graph::{AnimationGraph, AnimationState, AnimationStateId, GraphEvaluation, GraphLayer};
pub use ik::{AimIk, IkOutcome, TwoBoneIk};
pub use marker::{AnimationMarker, MarkerTrack};
pub use motion::{RootMotionTrack, RootMotionTransform};
pub use pose::Pose;
pub use types::{JointTransform, SamplingRatio};

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
