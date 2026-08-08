use std::sync::Arc;

use blackflower_animation_format::{
    AnimationContainer, RestTransform, RigJoint, SkeletonContainer, SkeletonIdentity,
};

use crate::error::map_native_failure;
use crate::ffi::{self, Status};
use crate::{AnimationClipDescriptor, AnimationMarker, Error, MarkerTrack, RootMotionTrack};
use crate::{JointTransform, SamplingRatio};

/// An immutable runtime skeleton loaded from a `.bfskel` asset.
#[derive(Debug)]
pub struct Skeleton {
    pub(crate) pointer: ffi::SkeletonPtr,
    pub(crate) identity: Arc<()>,
    skeleton_identity: SkeletonIdentity,
    names: Box<[String]>,
    parents: Box<[Option<usize>]>,
    rest_transforms: Box<[JointTransform]>,
}

impl Skeleton {
    /// Load and validate a cooked Blackflower skeleton.
    ///
    /// Raw `.ozz` archives are deliberately rejected by this runtime entry
    /// point.
    pub fn from_bytes(bytes: &[u8]) -> Result<Self, Error> {
        let container =
            SkeletonContainer::decode(bytes).map_err(|_error| Error::InvalidSkeletonArchive)?;
        validate_ozz_version(container.ozz_version())?;
        let skeleton = Self::from_ozz_bytes(container.ozz_skeleton())?;
        if skeleton.skeleton_identity != container.identity() {
            return Err(Error::SkeletonIdentityMismatch);
        }
        Ok(skeleton)
    }

    pub(crate) fn from_ozz_bytes(bytes: &[u8]) -> Result<Self, Error> {
        let pointer = ffi::load_skeleton(bytes)
            .map_err(|status| map_load(status, Error::InvalidSkeletonArchive))?;
        let mut skeleton = Self {
            pointer,
            identity: Arc::new(()),
            skeleton_identity: SkeletonIdentity::from_bytes([0; 32]),
            names: Box::new([]),
            parents: Box::new([]),
            rest_transforms: Box::new([]),
        };
        skeleton.load_metadata()?;
        Ok(skeleton)
    }

    /// Return the stable full-rig identity.
    #[must_use]
    pub const fn skeleton_identity(&self) -> SkeletonIdentity {
        self.skeleton_identity
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

    /// Return one joint-local rest transform.
    #[must_use]
    pub fn joint_rest_transform(&self, joint: usize) -> Option<JointTransform> {
        self.rest_transforms.get(joint).copied()
    }

    pub(crate) fn contains_joint(&self, joint: usize) -> bool {
        joint < self.joint_count()
    }

    pub(crate) fn is_ancestor(&self, ancestor: usize, descendant: usize) -> bool {
        let mut current = self.parents.get(descendant).copied().flatten();
        while let Some(joint) = current {
            if joint == ancestor {
                return true;
            }
            current = self.parents[joint];
        }
        false
    }

    fn load_metadata(&mut self) -> Result<(), Error> {
        let joint_count = usize::try_from(ffi::skeleton_joint_count(self.pointer))
            .map_err(|_error| Error::NativeContract)?;
        let names = load_joint_names(self.pointer, joint_count)?;
        let parents = load_joint_parents(self.pointer, joint_count)?;
        let native_rest =
            ffi::skeleton_rest_transforms(self.pointer, joint_count).map_err(map_native_failure)?;
        let rest_transforms = native_rest
            .iter()
            .map(joint_transform_from_native)
            .collect::<Vec<_>>();
        let rig = names
            .iter()
            .zip(&parents)
            .zip(&rest_transforms)
            .map(|((name, parent), rest)| RigJoint {
                name,
                parent: *parent,
                rest: RestTransform {
                    translation: rest.translation,
                    rotation: rest.rotation,
                    scale: rest.scale,
                },
            })
            .collect::<Vec<_>>();
        self.skeleton_identity =
            SkeletonIdentity::from_rig(&rig).map_err(|_error| Error::InvalidSkeletonArchive)?;
        self.names = names.into_boxed_slice();
        self.parents = parents.into_boxed_slice();
        self.rest_transforms = rest_transforms.into_boxed_slice();
        Ok(())
    }
}

impl Drop for Skeleton {
    fn drop(&mut self) {
        ffi::destroy_skeleton(self.pointer);
    }
}

/// An immutable runtime animation clip loaded from a `.bfanim` asset.
#[derive(Debug)]
pub struct Animation {
    pub(crate) pointer: ffi::AnimationPtr,
    pub(crate) identity: Arc<()>,
    descriptor: AnimationClipDescriptor,
    track_count: usize,
    markers: MarkerTrack,
    root_motion: Option<RootMotionTrack>,
}

impl Animation {
    /// Load and validate one cooked Blackflower animation clip.
    ///
    /// Raw `.ozz` archives are deliberately rejected by this runtime entry
    /// point.
    pub fn from_bytes(bytes: &[u8]) -> Result<Self, Error> {
        let container =
            AnimationContainer::decode(bytes).map_err(|_error| Error::InvalidAnimationArchive)?;
        validate_ozz_version(container.ozz_version())?;
        let metadata = container.metadata();
        let mut animation =
            Self::from_ozz_bytes(container.ozz_animation(), container.skeleton_identity())?;
        if animation.name() != metadata.name() {
            return Err(Error::AnimationNameMismatch);
        }
        animation.descriptor.looping = metadata.looping();
        animation.descriptor.additive = metadata.additive();
        animation.markers = MarkerTrack::new(
            metadata
                .markers()
                .iter()
                .map(|marker| {
                    SamplingRatio::new(marker.ratio())
                        .map(|ratio| AnimationMarker::new(marker.name(), ratio))
                })
                .collect::<Result<Vec<_>, _>>()?,
        )?;
        animation.root_motion = container
            .ozz_root_motion()
            .map(RootMotionTrack::from_ozz_bytes)
            .transpose()?;
        Ok(animation)
    }

    pub(crate) fn from_ozz_bytes(
        bytes: &[u8],
        skeleton_identity: SkeletonIdentity,
    ) -> Result<Self, Error> {
        let pointer = ffi::load_animation(bytes)
            .map_err(|status| map_load(status, Error::InvalidAnimationArchive))?;
        let duration = ffi::animation_duration(pointer);
        let track_count = usize::try_from(ffi::animation_track_count(pointer))
            .map_err(|_error| Error::NativeContract)?;
        let name = ffi::animation_name(pointer).map_err(map_native_failure)?;
        Ok(Self {
            pointer,
            identity: Arc::new(()),
            descriptor: AnimationClipDescriptor {
                name,
                skeleton_identity,
                duration,
                looping: false,
                additive: false,
            },
            track_count,
            markers: MarkerTrack::new([])?,
            root_motion: None,
        })
    }

    /// Return the descriptor stored by the cooked asset.
    #[must_use]
    pub const fn descriptor(&self) -> &AnimationClipDescriptor {
        &self.descriptor
    }

    /// Return the clip name.
    #[must_use]
    pub fn name(&self) -> &str {
        self.descriptor.name()
    }

    /// Return the clip duration in seconds.
    #[must_use]
    pub const fn duration(&self) -> f32 {
        self.descriptor.duration()
    }

    /// Return the required full-rig identity.
    #[must_use]
    pub const fn skeleton_identity(&self) -> SkeletonIdentity {
        self.descriptor.skeleton_identity()
    }

    /// Whether runtime playback loops.
    #[must_use]
    pub const fn looping(&self) -> bool {
        self.descriptor.looping()
    }

    /// Whether the clip contains additive transforms.
    #[must_use]
    pub const fn additive(&self) -> bool {
        self.descriptor.additive()
    }

    /// Return the number of animated tracks.
    #[must_use]
    pub const fn track_count(&self) -> usize {
        self.track_count
    }

    /// Return the clip's marker track.
    #[must_use]
    pub const fn markers(&self) -> &MarkerTrack {
        &self.markers
    }

    /// Return the optional extracted root-motion track.
    #[must_use]
    pub const fn root_motion(&self) -> Option<&RootMotionTrack> {
        self.root_motion.as_ref()
    }
}

impl Drop for Animation {
    fn drop(&mut self) {
        ffi::destroy_animation(self.pointer);
    }
}

fn load_joint_names(pointer: ffi::SkeletonPtr, count: usize) -> Result<Vec<String>, Error> {
    (0..count)
        .map(|joint| {
            let joint = u32::try_from(joint).map_err(|_error| Error::NativeContract)?;
            ffi::skeleton_joint_name(pointer, joint).map_err(map_native_failure)
        })
        .collect()
}

fn load_joint_parents(
    pointer: ffi::SkeletonPtr,
    count: usize,
) -> Result<Vec<Option<usize>>, Error> {
    (0..count)
        .map(|joint| {
            let native_joint = u32::try_from(joint).map_err(|_error| Error::NativeContract)?;
            let parent =
                ffi::skeleton_joint_parent(pointer, native_joint).map_err(map_native_failure)?;
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
        .collect()
}

fn joint_transform_from_native(transform: &ffi::Transform) -> JointTransform {
    JointTransform {
        translation: glam::Vec3::from_array(transform.translation),
        rotation: glam::Quat::from_array(transform.rotation),
        scale: glam::Vec3::from_array(transform.scale),
    }
}

fn validate_ozz_version(version: blackflower_animation_format::OzzVersion) -> Result<(), Error> {
    let expected = crate::ozz_version();
    if u32::from(version.major) == expected.0
        && u32::from(version.minor) == expected.1
        && u32::from(version.patch) == expected.2
    {
        Ok(())
    } else {
        Err(Error::UnsupportedOzzVersion)
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
