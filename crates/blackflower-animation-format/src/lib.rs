#![doc = include_str!("../README.md")]

mod container;
mod error;
mod identity;
mod metadata;

pub use container::{
    ANIMATION_MAGIC, AnimationContainer, CONTAINER_SCHEMA, HEADER_SIZE, OzzVersion, SKELETON_MAGIC,
    SkeletonContainer,
};
pub use error::Error;
pub use identity::{RestTransform, RigJoint, SkeletonIdentity};
pub use metadata::{ClipMarker, ClipMetadata};
