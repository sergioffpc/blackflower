use crate::{Error, Pose};

/// How one pose contributes to a blend operation.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum BlendMode {
    /// Normalized base layer.
    Normal,
    /// Additive delta applied after normal layers.
    Additive,
}

/// One immutable pose input to [`Pose::blend`].
#[derive(Debug, Clone, Copy)]
pub struct BlendLayer<'a> {
    pub(crate) pose: &'a Pose,
    pub(crate) weight: f32,
    pub(crate) joint_weights: Option<&'a [f32]>,
    pub(crate) mode: BlendMode,
}

impl<'a> BlendLayer<'a> {
    /// Construct a full-skeleton blend layer.
    pub fn new(pose: &'a Pose, weight: f32, mode: BlendMode) -> Result<Self, Error> {
        if !weight.is_finite() || weight < 0.0 {
            return Err(Error::InvalidBlendWeight);
        }
        Ok(Self {
            pose,
            weight,
            joint_weights: None,
            mode,
        })
    }

    /// Construct a normal blend layer.
    pub fn normal(pose: &'a Pose, weight: f32) -> Result<Self, Error> {
        Self::new(pose, weight, BlendMode::Normal)
    }

    /// Construct an additive blend layer.
    pub fn additive(pose: &'a Pose, weight: f32) -> Result<Self, Error> {
        Self::new(pose, weight, BlendMode::Additive)
    }

    /// Apply one scalar influence per skeleton joint.
    #[must_use]
    pub const fn with_joint_weights(mut self, joint_weights: &'a [f32]) -> Self {
        self.joint_weights = Some(joint_weights);
        self
    }

    /// Return the layer weight.
    #[must_use]
    pub const fn weight(self) -> f32 {
        self.weight
    }

    /// Return whether this is a normal or additive layer.
    #[must_use]
    pub const fn mode(self) -> BlendMode {
        self.mode
    }
}
