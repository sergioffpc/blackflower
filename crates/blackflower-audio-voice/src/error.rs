use thiserror::Error;

use crate::ffi;

/// Failure returned by the safe Opus API.
#[derive(Debug, Error, Clone, PartialEq, Eq)]
pub enum Error {
    #[error("Opus rejected an argument")]
    BadArgument,
    #[error("the Opus output buffer is too small")]
    BufferTooSmall,
    #[error("Opus reported an internal error")]
    Internal,
    #[error("the Opus packet is invalid or corrupted")]
    InvalidPacket,
    #[error("the requested Opus operation is not implemented")]
    Unimplemented,
    #[error("the Opus codec state is invalid")]
    InvalidState,
    #[error("Opus could not allocate codec state")]
    AllocationFailed,
    #[error("Opus returned unknown status code {0}")]
    UnknownStatus(i32),
    #[error("{buffer} must contain exactly {expected} samples, got {actual}")]
    FrameLength {
        buffer: &'static str,
        expected: usize,
        actual: usize,
    },
    #[error("{buffer} must contain complete interleaved frames for {channels} channels")]
    ChannelAlignment {
        buffer: &'static str,
        channels: usize,
    },
    #[error("{buffer} length {length} is outside the native Opus integer range")]
    LengthOutOfRange { buffer: &'static str, length: usize },
    #[error("{field} must be between {minimum} and {maximum}, got {actual}")]
    ConfigurationOutOfRange {
        field: &'static str,
        minimum: u32,
        maximum: u32,
        actual: u32,
    },
    #[error("an Opus packet cannot be empty")]
    EmptyPacket,
    #[error("Opus violated its documented API contract")]
    ContractViolation,
}

impl From<ffi::Status> for Error {
    fn from(status: ffi::Status) -> Self {
        match status {
            ffi::Status::BadArgument => Self::BadArgument,
            ffi::Status::BufferTooSmall => Self::BufferTooSmall,
            ffi::Status::Internal => Self::Internal,
            ffi::Status::InvalidPacket => Self::InvalidPacket,
            ffi::Status::Unimplemented => Self::Unimplemented,
            ffi::Status::InvalidState => Self::InvalidState,
            ffi::Status::AllocationFailed => Self::AllocationFailed,
            ffi::Status::Unknown(code) => Self::UnknownStatus(code),
            ffi::Status::ContractViolation => Self::ContractViolation,
        }
    }
}
