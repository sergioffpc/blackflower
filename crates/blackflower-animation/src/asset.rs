use std::sync::Arc;

use crate::Error;
use crate::error::map_native_failure;
use crate::ffi::{self, Status};

/// An immutable runtime skeleton loaded from an ozz archive.
#[derive(Debug)]
pub struct Skeleton {
    pub(crate) pointer: ffi::SkeletonPtr,
    pub(crate) identity: Arc<()>,
    names: Box<[String]>,
    parents: Box<[Option<usize>]>,
}

impl Skeleton {
    /// Load a runtime skeleton from trusted `.ozz` archive bytes.
    ///
    /// ozz archives are runtime assets produced by the matching ozz toolchain.
    /// They are not a sandboxed format and should not be accepted from
    /// untrusted sources.
    pub fn from_bytes(bytes: &[u8]) -> Result<Self, Error> {
        let pointer = ffi::load_skeleton(bytes)
            .map_err(|status| map_load(status, Error::InvalidSkeletonArchive))?;
        let mut skeleton = Self {
            pointer,
            identity: Arc::new(()),
            names: Box::new([]),
            parents: Box::new([]),
        };
        let joint_count = usize::try_from(ffi::skeleton_joint_count(pointer))
            .map_err(|_error| Error::NativeContract)?;

        let names = (0..joint_count)
            .map(|joint| {
                let joint = u32::try_from(joint).map_err(|_error| Error::NativeContract)?;
                ffi::skeleton_joint_name(pointer, joint).map_err(map_native_failure)
            })
            .collect::<Result<Vec<_>, _>>()?;
        let parents = (0..joint_count)
            .map(|joint| {
                let native_joint = u32::try_from(joint).map_err(|_error| Error::NativeContract)?;
                let parent = ffi::skeleton_joint_parent(pointer, native_joint)
                    .map_err(map_native_failure)?;
                if parent == -1 {
                    Ok(None)
                } else {
                    let parent =
                        usize::try_from(parent).map_err(|_error| Error::InvalidSkeletonArchive)?;
                    if parent < joint {
                        Ok(Some(parent))
                    } else {
                        Err(Error::InvalidSkeletonArchive)
                    }
                }
            })
            .collect::<Result<Vec<_>, _>>()?;

        skeleton.names = names.into_boxed_slice();
        skeleton.parents = parents.into_boxed_slice();
        Ok(skeleton)
    }

    /// Return the number of joints.
    #[must_use]
    pub fn joint_count(&self) -> usize {
        self.names.len()
    }

    /// Return a joint name, or `None` when the index is out of range.
    #[must_use]
    pub fn joint_name(&self, joint: usize) -> Option<&str> {
        self.names.get(joint).map(String::as_str)
    }

    /// Return a joint's parent index.
    ///
    /// The outer `None` means the joint index is out of range. The inner
    /// `None` identifies a root joint.
    #[must_use]
    pub fn joint_parent(&self, joint: usize) -> Option<Option<usize>> {
        self.parents.get(joint).copied()
    }
}

impl Drop for Skeleton {
    fn drop(&mut self) {
        ffi::destroy_skeleton(self.pointer);
    }
}

/// An immutable runtime animation clip loaded from an ozz archive.
#[derive(Debug)]
pub struct Animation {
    pub(crate) pointer: ffi::AnimationPtr,
    pub(crate) identity: Arc<()>,
    name: String,
    duration: f32,
    track_count: usize,
}

impl Animation {
    /// Load a runtime animation clip from trusted `.ozz` archive bytes.
    ///
    /// ozz archives are runtime assets produced by the matching ozz toolchain.
    /// They are not a sandboxed format and should not be accepted from
    /// untrusted sources.
    pub fn from_bytes(bytes: &[u8]) -> Result<Self, Error> {
        let pointer = ffi::load_animation(bytes)
            .map_err(|status| map_load(status, Error::InvalidAnimationArchive))?;
        let mut animation = Self {
            pointer,
            identity: Arc::new(()),
            name: String::new(),
            duration: ffi::animation_duration(pointer),
            track_count: 0,
        };
        animation.track_count = usize::try_from(ffi::animation_track_count(pointer))
            .map_err(|_error| Error::NativeContract)?;
        animation.name = ffi::animation_name(pointer).map_err(map_native_failure)?;
        Ok(animation)
    }

    /// Return the clip name stored by ozz.
    #[must_use]
    pub fn name(&self) -> &str {
        &self.name
    }

    /// Return the clip duration in seconds.
    #[must_use]
    pub const fn duration(&self) -> f32 {
        self.duration
    }

    /// Return the number of animated tracks.
    #[must_use]
    pub const fn track_count(&self) -> usize {
        self.track_count
    }
}

impl Drop for Animation {
    fn drop(&mut self) {
        ffi::destroy_animation(self.pointer);
    }
}

const fn map_load(status: Status, invalid_archive: Error) -> Error {
    match status {
        Status::InvalidArgument | Status::InvalidArchive => invalid_archive,
        Status::OutOfMemory
        | Status::Incompatible
        | Status::JobFailed
        | Status::IndexOutOfRange
        | Status::NativeFailure
        | Status::ContractViolation => map_native_failure(status),
    }
}
