//! Host-only inspection of private ozz payloads produced during cooking.

use blackflower_animation_format::SkeletonIdentity;

use crate::{Animation, Error, Skeleton};

/// Metadata extracted from a private ozz skeleton payload.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub struct SkeletonInspection {
    /// Full ordered rig identity.
    pub identity: SkeletonIdentity,
    /// Number of joints.
    pub joint_count: usize,
}

/// Metadata extracted from a private ozz animation payload.
#[derive(Debug, Clone, PartialEq)]
pub struct AnimationInspection {
    /// Clip name.
    pub name: String,
    /// Duration in seconds.
    pub duration: f32,
    /// Number of animated tracks.
    pub track_count: usize,
}

/// Inspect a raw ozz skeleton emitted in a temporary cooking directory.
pub fn inspect_skeleton_ozz(bytes: &[u8]) -> Result<SkeletonInspection, Error> {
    let skeleton = Skeleton::from_ozz_bytes(bytes)?;
    Ok(SkeletonInspection {
        identity: skeleton.skeleton_identity(),
        joint_count: skeleton.joint_count(),
    })
}

/// Inspect a raw ozz animation emitted in a temporary cooking directory.
pub fn inspect_animation_ozz(bytes: &[u8]) -> Result<AnimationInspection, Error> {
    let animation = Animation::from_ozz_bytes(bytes, SkeletonIdentity::from_bytes([0; 32]))?;
    Ok(AnimationInspection {
        name: animation.name().to_owned(),
        duration: animation.duration(),
        track_count: animation.track_count(),
    })
}

#[cfg(test)]
#[path = "../tests/unit/cooking.rs"]
mod tests;
