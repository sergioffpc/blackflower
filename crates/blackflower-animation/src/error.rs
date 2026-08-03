/// Errors produced while loading or evaluating skeletal animation.
#[derive(Debug, Clone, PartialEq, Eq, thiserror::Error)]
pub enum Error {
    /// The bytes are not a supported runtime skeleton archive.
    #[error("invalid Blackflower skeleton asset")]
    InvalidSkeletonArchive,
    /// The bytes are not a supported runtime animation archive.
    #[error("invalid Blackflower animation asset")]
    InvalidAnimationArchive,
    /// A container requires a different ozz runtime version.
    #[error("animation asset requires an unsupported ozz-animation version")]
    UnsupportedOzzVersion,
    /// The skeleton contents do not match the container or clip identity.
    #[error("animation and skeleton identities do not match")]
    SkeletonIdentityMismatch,
    /// The typed clip name differs from the private ozz payload.
    #[error("animation metadata name does not match the ozz clip name")]
    AnimationNameMismatch,
    /// The root-motion section is not a supported pair of ozz tracks.
    #[error("invalid ozz root-motion archive")]
    InvalidRootMotionArchive,
    /// Root-motion traversal moved backwards without wrapping.
    #[error("root-motion traversal moved backwards without wrapping")]
    InvalidRootMotionTraversal,
    /// A runtime set cannot contain clips for different skeletons.
    #[error("animation set and clip skeleton identities do not match")]
    AnimationSetSkeletonMismatch,
    /// A runtime set already contains the clip name.
    #[error("animation set already contains the clip name")]
    DuplicateAnimationClip,
    /// Native allocation failed.
    #[error("ozz-animation native allocation failed")]
    OutOfMemory,
    /// A sampling context must support at least one track and fit the native API.
    #[error("invalid sampling context capacity {0}")]
    InvalidContextCapacity(usize),
    /// A sampling ratio must be finite and between zero and one.
    #[error("sampling ratio must be finite and in the inclusive range 0..=1")]
    InvalidSamplingRatio,
    /// The pose was allocated for another skeleton.
    #[error("pose belongs to another skeleton")]
    WrongSkeleton,
    /// The animation tracks do not match the skeleton joints.
    #[error("animation has {tracks} tracks but skeleton has {joints} joints")]
    TrackCountMismatch {
        /// Skeleton joint count.
        joints: usize,
        /// Animation track count.
        tracks: usize,
    },
    /// The sampling context cannot hold all animation tracks.
    #[error("sampling context holds {capacity} tracks but animation requires {required}")]
    ContextTooSmall {
        /// Animation track count.
        required: usize,
        /// Context track capacity.
        capacity: usize,
    },
    /// A blend layer weight must be finite and non-negative.
    #[error("blend layer weight must be finite and non-negative")]
    InvalidBlendWeight,
    /// The blend threshold must be finite and greater than zero.
    #[error("blend threshold must be finite and greater than zero")]
    InvalidBlendThreshold,
    /// A partial blend layer supplied the wrong number of joint weights.
    #[error("blend layer has {actual} joint weights but skeleton has {expected} joints")]
    JointWeightCountMismatch {
        /// Skeleton joint count.
        expected: usize,
        /// Supplied joint weight count.
        actual: usize,
    },
    /// A per-joint blend weight must be finite and in the inclusive range `0..=1`.
    #[error("invalid blend weight for joint {joint}")]
    InvalidJointWeight {
        /// Joint whose weight was invalid.
        joint: usize,
    },
    /// A complete local pose must contain one transform per skeleton joint.
    #[error("local pose has {actual} transforms but skeleton has {expected} joints")]
    LocalTransformCountMismatch {
        /// Skeleton joint count.
        expected: usize,
        /// Supplied transform count.
        actual: usize,
    },
    /// A local transform contained non-finite values or a non-normalized rotation.
    #[error("invalid local transform for joint {joint}")]
    InvalidJointTransform {
        /// Joint whose transform was invalid.
        joint: usize,
    },
    /// A joint index was outside the skeleton.
    #[error("joint index {joint} is outside skeleton with {joint_count} joints")]
    JointIndexOutOfRange {
        /// Invalid joint index.
        joint: usize,
        /// Skeleton joint count.
        joint_count: usize,
    },
    /// An inverse-kinematics parameter was non-finite, degenerate, or out of range.
    #[error("invalid inverse-kinematics configuration")]
    InvalidIkConfiguration,
    /// A two-bone inverse-kinematics chain did not follow the skeleton hierarchy.
    #[error("invalid two-bone inverse-kinematics joint chain")]
    InvalidIkChain,
    /// An animation state duration must be finite and greater than zero.
    #[error("animation state duration must be finite and greater than zero")]
    InvalidStateDuration,
    /// Animation playback speed must be finite and non-negative.
    #[error("animation playback speed must be finite and non-negative")]
    InvalidPlaybackSpeed,
    /// Animation graph advancement requires a finite, non-negative delta.
    #[error("animation graph delta must be finite and non-negative")]
    InvalidGraphDelta,
    /// An animation graph state identifier was not registered.
    #[error("unknown animation state {0}")]
    UnknownAnimationState(usize),
    /// The same directed animation transition was registered more than once.
    #[error("animation transition is already registered")]
    DuplicateTransition,
    /// No directed transition exists from the current state to the requested state.
    #[error("animation transition is not registered")]
    MissingTransition,
    /// A new transition cannot begin before the active transition completes.
    #[error("animation transition is already in progress")]
    TransitionInProgress,
    /// A transition duration must be finite and non-negative.
    #[error("animation transition duration must be finite and non-negative")]
    InvalidTransitionDuration,
    /// Animation markers must be ordered by non-decreasing normalized time.
    #[error("animation markers are not ordered by normalized time")]
    InvalidMarkerOrder,
    /// A marker traversal without wrapping cannot move backwards.
    #[error("animation marker traversal moved backwards without wrapping")]
    InvalidMarkerTraversal,
    /// ozz-animation rejected a configured sampling, blending, IK, or local-to-model job.
    #[error("ozz-animation native job failed")]
    NativeJobFailed,
    /// An unexpected C++ failure was contained by the native wrapper.
    #[error("ozz-animation native runtime failed")]
    NativeFailure,
    /// The private native wrapper rejected an internal call.
    #[error("native ozz-animation wrapper contract violation")]
    NativeContract,
}

pub(crate) const fn map_native_failure(status: crate::ffi::Status) -> Error {
    match status {
        crate::ffi::Status::OutOfMemory => Error::OutOfMemory,
        crate::ffi::Status::NativeFailure => Error::NativeFailure,
        crate::ffi::Status::InvalidArgument
        | crate::ffi::Status::InvalidArchive
        | crate::ffi::Status::Incompatible
        | crate::ffi::Status::JobFailed
        | crate::ffi::Status::IndexOutOfRange
        | crate::ffi::Status::ContractViolation => Error::NativeContract,
    }
}
