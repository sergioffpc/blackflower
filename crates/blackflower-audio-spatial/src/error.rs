use crate::{RayTracerBackend, ffi::Status};

/// Errors produced while initializing or using Steam Audio.
#[derive(Debug, thiserror::Error)]
pub enum Error {
    /// The selected ray tracer was not compiled for this target.
    #[error("Steam Audio ray tracer {backend:?} is unavailable on this target")]
    RayTracerUnavailable {
        /// Requested backend.
        backend: RayTracerBackend,
    },
    /// Steam Audio only serializes scenes using its built-in ray tracer.
    #[error("Steam Audio scene serialization requires the built-in ray tracer")]
    SceneSerializationRequiresBuiltIn,
    /// Steam Audio rejected an operation.
    #[error("Steam Audio operation {operation} failed")]
    NativeFailure {
        /// Name of the rejected operation.
        operation: &'static str,
    },
    /// Steam Audio could not allocate native memory.
    #[error("Steam Audio operation {operation} ran out of memory")]
    OutOfMemory {
        /// Name of the rejected operation.
        operation: &'static str,
    },
    /// Steam Audio could not initialize an external dependency.
    #[error("Steam Audio operation {operation} could not initialize a dependency")]
    NativeInitialization {
        /// Name of the rejected operation.
        operation: &'static str,
    },
    /// Steam Audio returned a value outside its documented contract.
    #[error("Steam Audio violated the native API contract during {operation}")]
    NativeContract {
        /// Name of the operation.
        operation: &'static str,
    },
    /// Sampling rate and frame size must be non-zero.
    #[error("{field} must be non-zero")]
    ZeroAudioSetting {
        /// Setting name.
        field: &'static str,
    },
    /// Sampling rate or frame size is outside the native signed 32-bit range.
    #[error("{field} value {value} exceeds the Steam Audio API range")]
    AudioSettingOutOfRange {
        /// Setting name.
        field: &'static str,
        /// Rejected value.
        value: u32,
    },
    /// A source direction must be finite and non-zero.
    #[error("binaural source direction must be finite and non-zero")]
    InvalidDirection,
    /// Spatial blend must be a finite value between zero and one.
    #[error("spatial blend must be finite and between 0 and 1")]
    InvalidSpatialBlend,
    /// A processing buffer does not match the configured frame size.
    #[error("{buffer} contains {actual} samples; expected {expected}")]
    FrameLength {
        /// Buffer role.
        buffer: &'static str,
        /// Configured frame size.
        expected: usize,
        /// Supplied length.
        actual: usize,
    },
    /// An HRTF belongs to another Steam Audio context.
    #[error("HRTF belongs to another Steam Audio context")]
    WrongContext,
    /// An acoustic material coefficient is not a finite fraction.
    #[error("acoustic material coefficients must be finite values between 0 and 1")]
    InvalidAcousticMaterial,
    /// Acoustic scene geometry is empty or contains invalid indices or vertices.
    #[error("invalid acoustic scene geometry")]
    InvalidSceneGeometry,
    /// Acoustic scene geometry exceeds Steam Audio's signed 32-bit limits.
    #[error("acoustic scene geometry exceeds Steam Audio limits")]
    SceneGeometryCountOutOfRange,
    /// A cooked acoustic asset has an invalid header, body, checksum, or semantic value.
    #[error("invalid {format} asset: {reason}")]
    InvalidAcousticAsset {
        /// Short format name.
        format: &'static str,
        /// Validation failure.
        reason: &'static str,
    },
    /// Probe placement or bake quality settings are invalid.
    #[error("invalid acoustic probe or bake settings")]
    InvalidProbeSettings,
    /// A scene or probe batch belongs to another Steam Audio context.
    #[error("acoustic object belongs to another Steam Audio context")]
    WrongAcousticContext,
    /// An environmental effect was configured with an empty frame.
    #[error("environmental effect frame size must be non-zero")]
    InvalidEffectFrame,
    /// A reflection update capacity or crossfade duration is invalid.
    #[error("invalid reflection simulator settings")]
    InvalidReflectionSettings,
}

impl Error {
    pub(crate) const fn from_status(operation: &'static str, status: Status) -> Self {
        match status {
            Status::Failure => Self::NativeFailure { operation },
            Status::OutOfMemory => Self::OutOfMemory { operation },
            Status::Initialization => Self::NativeInitialization { operation },
            Status::ContractViolation => Self::NativeContract { operation },
        }
    }
}
