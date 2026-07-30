use std::collections::BTreeMap;

use blackflower_animation_format::SkeletonIdentity;

use crate::{Animation, Error};

/// Immutable metadata needed to select and evaluate one animation clip.
#[derive(Debug, Clone, PartialEq)]
pub struct AnimationClipDescriptor {
    pub(crate) name: String,
    pub(crate) skeleton_identity: SkeletonIdentity,
    pub(crate) duration: f32,
    pub(crate) looping: bool,
    pub(crate) additive: bool,
}

impl AnimationClipDescriptor {
    /// Stable clip name.
    #[must_use]
    pub fn name(&self) -> &str {
        &self.name
    }

    /// Required skeleton identity.
    #[must_use]
    pub const fn skeleton_identity(&self) -> SkeletonIdentity {
        self.skeleton_identity
    }

    /// Duration in seconds.
    #[must_use]
    pub const fn duration(&self) -> f32 {
        self.duration
    }

    /// Whether playback loops.
    #[must_use]
    pub const fn looping(&self) -> bool {
        self.looping
    }

    /// Whether transforms are additive.
    #[must_use]
    pub const fn additive(&self) -> bool {
        self.additive
    }
}

/// Deterministically keyed runtime collection of clips for one skeleton.
#[derive(Debug)]
pub struct AnimationSet {
    skeleton_identity: SkeletonIdentity,
    clips: BTreeMap<String, Animation>,
}

impl AnimationSet {
    /// Construct an empty set for one full-rig identity.
    #[must_use]
    pub const fn new(skeleton_identity: SkeletonIdentity) -> Self {
        Self {
            skeleton_identity,
            clips: BTreeMap::new(),
        }
    }

    /// Insert one uniquely named clip with the same required skeleton.
    pub fn insert(&mut self, animation: Animation) -> Result<(), Error> {
        if animation.skeleton_identity() != self.skeleton_identity {
            return Err(Error::AnimationSetSkeletonMismatch);
        }
        if self.clips.contains_key(animation.name()) {
            return Err(Error::DuplicateAnimationClip);
        }
        self.clips.insert(animation.name().to_owned(), animation);
        Ok(())
    }

    /// Return the set's required skeleton identity.
    #[must_use]
    pub const fn skeleton_identity(&self) -> SkeletonIdentity {
        self.skeleton_identity
    }

    /// Find a clip by its stable name.
    #[must_use]
    pub fn get(&self, name: &str) -> Option<&Animation> {
        self.clips.get(name)
    }

    /// Number of clips.
    #[must_use]
    pub fn len(&self) -> usize {
        self.clips.len()
    }

    /// Whether the set contains no clips.
    #[must_use]
    pub fn is_empty(&self) -> bool {
        self.clips.is_empty()
    }

    /// Iterate in stable clip-name order.
    pub fn iter(&self) -> impl ExactSizeIterator<Item = (&str, &Animation)> {
        self.clips
            .iter()
            .map(|(name, animation)| (name.as_str(), animation))
    }
}
