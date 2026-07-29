/// Errors produced while loading or sampling ozz-animation assets.
#[derive(Debug, Clone, PartialEq, Eq, thiserror::Error)]
pub enum Error {
    /// The bytes are not a supported runtime skeleton archive.
    #[error("invalid ozz-animation skeleton archive")]
    InvalidSkeletonArchive,
    /// The bytes are not a supported runtime animation archive.
    #[error("invalid ozz-animation clip archive")]
    InvalidAnimationArchive,
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
    /// ozz-animation rejected a configured sampling or local-to-model job.
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
