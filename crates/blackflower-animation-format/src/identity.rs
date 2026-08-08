use std::fmt;

use glam::{Quat, Vec3};

use crate::Error;

const IDENTITY_DOMAIN: &[u8] = b"blackflower.skeleton-identity.v1";
const SIGN_BIT: u32 = 1 << 31;
const MAGNITUDE_BITS: u32 = !SIGN_BIT;

/// Stable identity of an ordered skeleton hierarchy and its rest pose.
#[derive(Debug, Clone, Copy, PartialEq, Eq, PartialOrd, Ord, Hash)]
pub struct SkeletonIdentity([u8; 32]);

impl SkeletonIdentity {
    /// Construct an identity from its canonical bytes.
    #[must_use]
    pub const fn from_bytes(bytes: [u8; 32]) -> Self {
        Self(bytes)
    }

    /// Return the canonical identity bytes.
    #[must_use]
    pub const fn as_bytes(&self) -> &[u8; 32] {
        &self.0
    }

    /// Hash an ordered rig definition.
    pub fn from_rig(joints: &[RigJoint<'_>]) -> Result<Self, Error> {
        let joint_count = u32::try_from(joints.len()).map_err(|_error| Error::InvalidRig)?;
        if joint_count == 0 {
            return Err(Error::InvalidRig);
        }

        let mut hasher = blake3::Hasher::new();
        hasher.update(IDENTITY_DOMAIN);
        hasher.update(&joint_count.to_le_bytes());
        for (index, joint) in joints.iter().enumerate() {
            validate_joint(index, joint)?;
            hash_text(&mut hasher, joint.name)?;
            let parent = match joint.parent {
                Some(parent) => i32::try_from(parent).map_err(|_error| Error::InvalidRig)?,
                None => -1,
            };
            hasher.update(&parent.to_le_bytes());
            hash_transform(&mut hasher, joint.rest)?;
        }
        Ok(Self(*hasher.finalize().as_bytes()))
    }
}

impl fmt::Display for SkeletonIdentity {
    fn fmt(&self, formatter: &mut fmt::Formatter<'_>) -> fmt::Result {
        for byte in self.0 {
            write!(formatter, "{byte:02x}")?;
        }
        Ok(())
    }
}

/// One joint participating in the canonical skeleton identity.
#[derive(Debug, Clone, Copy)]
pub struct RigJoint<'a> {
    /// Stable joint name.
    pub name: &'a str,
    /// Parent joint index, or `None` for a root.
    pub parent: Option<usize>,
    /// Joint-local rest transform.
    pub rest: RestTransform,
}

/// Joint-local translation, rotation, and scale in the skeleton rest pose.
#[derive(Debug, Clone, Copy, PartialEq)]
pub struct RestTransform {
    /// Joint-local translation.
    pub translation: Vec3,
    /// Joint-local orientation.
    pub rotation: Quat,
    /// Joint-local scale.
    pub scale: Vec3,
}

fn validate_joint(index: usize, joint: &RigJoint<'_>) -> Result<(), Error> {
    if joint.name.is_empty()
        || joint.name.chars().any(char::is_control)
        || joint.parent.is_some_and(|parent| parent >= index)
    {
        return Err(Error::InvalidRig);
    }
    Ok(())
}

fn hash_text(hasher: &mut blake3::Hasher, value: &str) -> Result<(), Error> {
    let length = u32::try_from(value.len()).map_err(|_error| Error::InvalidRig)?;
    hasher.update(&length.to_le_bytes());
    hasher.update(value.as_bytes());
    Ok(())
}

fn hash_transform(hasher: &mut blake3::Hasher, transform: RestTransform) -> Result<(), Error> {
    for value in transform.translation.to_array() {
        hash_float(hasher, value)?;
    }

    let mut rotation = transform.rotation;
    if quaternion_needs_flip(rotation) {
        rotation = -rotation;
    }
    for value in rotation.to_array() {
        hash_float(hasher, value)?;
    }
    for value in transform.scale.to_array() {
        hash_float(hasher, value)?;
    }
    Ok(())
}

fn hash_float(hasher: &mut blake3::Hasher, value: f32) -> Result<(), Error> {
    if !value.is_finite() {
        return Err(Error::InvalidRig);
    }
    let bits = value.to_bits();
    let canonical = if bits & MAGNITUDE_BITS == 0 { 0 } else { bits };
    hasher.update(&canonical.to_le_bytes());
    Ok(())
}

fn quaternion_needs_flip(rotation: Quat) -> bool {
    for value in [rotation.w, rotation.x, rotation.y, rotation.z] {
        let bits = value.to_bits();
        if bits & MAGNITUDE_BITS != 0 {
            return bits & SIGN_BIT != 0;
        }
    }
    false
}

#[cfg(test)]
#[path = "../tests/unit/identity.rs"]
mod tests;
