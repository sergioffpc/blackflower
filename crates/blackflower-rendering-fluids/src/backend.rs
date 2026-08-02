use crate::{
    BackendError, BufferDesc, BufferTextureCopyPass, ComputePass, ComputePipelineDesc,
    CopyBufferPass, CopyTexturePass, Feature, ResourceId, SamplerDesc, TextureDesc,
};

/// Renderer operations needed by `NvFlowContextOpt`.
///
/// Backend implementations own all resources behind [`ResourceId`] and encode
/// passes into their renderer command stream. Defaults reject optional
/// operations so a backend-validation probe can be implemented before the
/// complete Grid shader surface.
pub trait Backend {
    /// Monotonic frame currently being encoded.
    fn current_frame(&self) -> u64;

    /// Latest GPU submission known to have completed.
    fn last_completed_frame(&self) -> u64;

    /// Reports optional aliasing or external-handle support.
    fn feature_supported(&self, _feature: Feature) -> bool {
        false
    }

    fn create_buffer(&mut self, desc: BufferDesc) -> Result<ResourceId, BackendError>;
    fn destroy_buffer(&mut self, buffer: ResourceId) -> Result<(), BackendError>;
    fn map_buffer(&mut self, buffer: ResourceId) -> Result<&mut [u8], BackendError>;
    fn unmap_buffer(&mut self, buffer: ResourceId) -> Result<(), BackendError>;

    fn create_texture(&mut self, _desc: TextureDesc) -> Result<ResourceId, BackendError> {
        Err(BackendError::Unsupported("create_texture"))
    }

    fn destroy_texture(&mut self, _texture: ResourceId) -> Result<(), BackendError> {
        Err(BackendError::Unsupported("destroy_texture"))
    }

    fn create_sampler(&mut self, _desc: SamplerDesc) -> Result<ResourceId, BackendError> {
        Err(BackendError::Unsupported("create_sampler"))
    }

    fn destroy_sampler(&mut self, _sampler: ResourceId) -> Result<(), BackendError> {
        Err(BackendError::Unsupported("destroy_sampler"))
    }

    fn create_compute_pipeline(
        &mut self,
        _desc: ComputePipelineDesc<'_>,
    ) -> Result<ResourceId, BackendError> {
        Err(BackendError::Unsupported("create_compute_pipeline"))
    }

    fn destroy_compute_pipeline(&mut self, _pipeline: ResourceId) -> Result<(), BackendError> {
        Err(BackendError::Unsupported("destroy_compute_pipeline"))
    }

    fn add_compute_pass(&mut self, _pass: ComputePass<'_>) -> Result<(), BackendError> {
        Err(BackendError::Unsupported("add_compute_pass"))
    }

    fn add_copy_buffer_pass(&mut self, _pass: CopyBufferPass<'_>) -> Result<(), BackendError> {
        Err(BackendError::Unsupported("add_copy_buffer_pass"))
    }

    fn add_copy_buffer_to_texture_pass(
        &mut self,
        _pass: BufferTextureCopyPass<'_>,
    ) -> Result<(), BackendError> {
        Err(BackendError::Unsupported("add_copy_buffer_to_texture_pass"))
    }

    fn add_copy_texture_to_buffer_pass(
        &mut self,
        _pass: BufferTextureCopyPass<'_>,
    ) -> Result<(), BackendError> {
        Err(BackendError::Unsupported("add_copy_texture_to_buffer_pass"))
    }

    fn add_copy_texture_pass(&mut self, _pass: CopyTexturePass<'_>) -> Result<(), BackendError> {
        Err(BackendError::Unsupported("add_copy_texture_pass"))
    }
}
