/// Errors produced while configuring or using a physics world.
#[derive(Debug, Clone, PartialEq, Eq, thiserror::Error)]
pub enum Error {
    /// Jolt could not allocate or initialize a physics world.
    #[error("Jolt physics world initialization failed")]
    WorldInitialization,
    /// The world configuration exceeds a Jolt limit.
    #[error("invalid Jolt physics world configuration")]
    InvalidWorldConfiguration,
    /// A worker count does not fit the Jolt C++ API.
    #[error("worker count {0} exceeds the Jolt limit")]
    WorkerCountTooLarge(u32),
    /// A collision-step count does not fit the Jolt C++ API.
    #[error("collision-step count {0} exceeds the Jolt limit")]
    CollisionStepCountTooLarge(u32),
    /// A step delta must be finite and strictly positive.
    #[error("step delta must be finite and strictly positive")]
    InvalidStepDelta,
    /// A vector must contain only finite components.
    #[error("vector components must be finite")]
    InvalidVector,
    /// A quaternion must be finite and normalized.
    #[error("quaternion must be finite and normalized")]
    InvalidRotation,
    /// A shape dimension must be finite and strictly positive.
    #[error("shape dimensions must be finite and strictly positive")]
    InvalidShape,
    /// A body handle belongs to another physics world.
    #[error("body handle belongs to another physics world")]
    WrongWorld,
    /// A body handle is stale or no longer exists.
    #[error("body no longer exists")]
    BodyNotFound,
    /// The configured body capacity has been exhausted.
    #[error("physics world body capacity exhausted")]
    BodyCapacityExhausted,
    /// Jolt reported one or more capacity errors while stepping.
    #[error(transparent)]
    Update(#[from] UpdateError),
    /// The private native wrapper rejected an internal call.
    #[error("native Jolt wrapper contract violation")]
    NativeContract,
}

/// Capacity errors reported by one Jolt simulation update.
#[derive(Debug, Clone, Copy, PartialEq, Eq, thiserror::Error)]
#[error("Jolt update failed with flags {bits:#010x}")]
pub struct UpdateError {
    bits: u32,
}

impl UpdateError {
    pub(crate) const fn new(bits: u32) -> Self {
        Self { bits }
    }

    /// Return Jolt's complete update error bit field.
    #[must_use]
    pub const fn bits(self) -> u32 {
        self.bits
    }

    /// The contact manifold cache was full.
    #[must_use]
    pub const fn manifold_cache_full(self) -> bool {
        self.bits & crate::ffi::raw::BF_PHYSICS_UPDATE_MANIFOLD_CACHE_FULL != 0
    }

    /// The broad-phase body-pair cache was full.
    #[must_use]
    pub const fn body_pair_cache_full(self) -> bool {
        self.bits & crate::ffi::raw::BF_PHYSICS_UPDATE_BODY_PAIR_CACHE_FULL != 0
    }

    /// The contact-constraint buffer was full.
    #[must_use]
    pub const fn contact_constraints_full(self) -> bool {
        self.bits & crate::ffi::raw::BF_PHYSICS_UPDATE_CONTACT_CONSTRAINTS_FULL != 0
    }
}
