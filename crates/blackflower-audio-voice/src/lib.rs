#![doc = include_str!("../README.md")]

mod decoder;
mod encoder;
mod error;
mod ffi;
mod types;

pub use decoder::Decoder;
pub use encoder::Encoder;
pub use error::Error;
pub use types::{Application, Channels, FrameDuration, SampleRate};

/// The Opus version whose source and headers are pinned.
pub const OPUS_VERSION: (u32, u32, u32) = (1, 5, 2);

/// Return the version string reported by the statically linked Opus library.
pub fn version_string() -> Result<&'static str, Error> {
    ffi::version_string().map_err(Error::from)
}
