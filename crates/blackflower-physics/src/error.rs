/// Errors produced while configuring or using a physics world.
#[derive(Debug, Clone, PartialEq, Eq, thiserror::Error)]
pub enum Error {
    /// Jolt could not initialize a physics world.
    #[error("Jolt physics world initialization failed")]
    WorldInitialization,
    /// A native physics allocation failed.
    #[error("native Jolt allocation failed")]
    OutOfMemory,
    /// Jolt or the native wrapper raised an unexpected exception.
    #[error("unexpected native Jolt failure")]
    NativeFailure,
    /// The Jolt archive and wrapper headers were built with different configurations.
    #[error("native Jolt configuration does not match the wrapper headers")]
    NativeConfigurationMismatch,
    /// The world configuration exceeds a Jolt limit.
    #[error("invalid Jolt physics world configuration")]
    InvalidWorldConfiguration,
    /// A worker count does not fit the Jolt C++ API.
    #[error("worker count {0} exceeds the Jolt limit")]
    WorkerCountTooLarge(u32),
    /// A collision-step count does not fit the Jolt C++ API.
    #[error("collision-step count {0} exceeds the Jolt limit")]
    CollisionStepCountTooLarge(u32),
    /// A vector must contain only finite components.
    #[error("vector components must be finite")]
    InvalidVector,
    /// A ray segment must have finite endpoints and non-zero length.
    #[error("ray segment must have finite endpoints and non-zero length")]
    InvalidRay,
    /// A quaternion must be finite and normalized.
    #[error("quaternion must be finite and normalized")]
    InvalidRotation,
    /// A collision-shape description has invalid dimensions or topology.
    #[error("invalid collision-shape description")]
    InvalidShape,
    /// Jolt could not build collision geometry from a validated shape description.
    #[error("Jolt could not create collision geometry from the shape description")]
    ShapeCreationFailed,
    /// Triangle meshes and compounds containing them may only be static bodies.
    #[error("triangle mesh collision geometry requires a static body")]
    StaticShapeRequiresStaticBody,
    /// The rigid-body character controller requires a capsule shape.
    #[error("character controller requires a capsule shape")]
    InvalidCharacterShape,
    /// Character mass must be finite and strictly positive.
    #[error("character mass must be finite and strictly positive")]
    InvalidCharacterMass,
    /// Character friction must be finite and non-negative.
    #[error("character friction must be finite and non-negative")]
    InvalidCharacterFriction,
    /// Character gravity factor must be finite.
    #[error("character gravity factor must be finite")]
    InvalidCharacterGravityFactor,
    /// Character slope angle must be finite and between zero and pi over two.
    #[error("character slope angle must be between zero and pi over two")]
    InvalidCharacterSlopeAngle,
    /// Ground separation must be finite and non-negative.
    #[error("character ground separation must be finite and non-negative")]
    InvalidCharacterGroundSeparation,
    /// A body handle belongs to another physics world.
    #[error("body handle belongs to another physics world")]
    WrongWorld,
    /// A character handle belongs to another physics world.
    #[error("character handle belongs to another physics world")]
    WrongCharacterWorld,
    /// A body handle is stale or no longer exists.
    #[error("body no longer exists")]
    BodyNotFound,
    /// A character handle is stale or no longer exists.
    #[error("character controller no longer exists")]
    CharacterNotFound,
    /// A body must be destroyed through its owning character controller.
    #[error("body is owned by a character controller")]
    BodyOwnedByCharacter,
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
