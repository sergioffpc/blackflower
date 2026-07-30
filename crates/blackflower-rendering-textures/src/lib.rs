//! KTX2 texture cooking, validation, and capability-driven runtime transcoding.
//!
//! Authoring image decoding and GPU resource creation remain outside this
//! crate. The safe API accepts canonical mip bytes, produces KTX2, and turns
//! authenticated KTX2 assets into upload-ready texture payloads.

mod error;
mod ffi;
mod texture;

pub use error::Error;
pub use texture::{
    EncodeOptions, TextureAsset, TextureFormat, TextureMip, TextureQuality, TextureSemantic,
    TextureTargetCapabilities, TranscodedMip, TranscodedTexture, encode, ktx_version,
};

/// KTX-Software release pinned by this crate.
pub const KTX_SOFTWARE_VERSION: &str = env!("BLACKFLOWER_KTX_VERSION");
