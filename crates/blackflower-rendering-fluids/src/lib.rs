#![doc = include_str!("../README.md")]

mod backend;
mod context;
mod error;
mod ffi;
mod types;
mod wgpu_backend;

pub use backend::Backend;
pub use context::Context;
pub use error::{BackendError, Error};
pub use types::{
    AddressMode, BindingDesc, BufferDesc, BufferTextureCopyPass, BufferUsage, ComputePass,
    ComputePipelineDesc, CopyBufferPass, CopyTexturePass, DescriptorType, Feature, FilterMode,
    Format, MemoryType, ResourceBinding, ResourceId, SamplerDesc, TextureDesc, TextureType,
    TextureUsage,
};
pub use wgpu_backend::WgpuBackend;

/// The NVIDIA Flow version compiled into this crate.
#[must_use]
pub fn flow_version() -> &'static str {
    ffi::flow_version()
}
