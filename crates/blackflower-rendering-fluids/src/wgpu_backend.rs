use std::borrow::Cow;
use std::collections::{BTreeMap, BTreeSet};
use std::sync::{Arc, mpsc};

use crate::{
    AddressMode, Backend, BackendError, BufferDesc, BufferTextureCopyPass, BufferUsage,
    ComputePass, ComputePipelineDesc, CopyBufferPass, CopyTexturePass, DescriptorType, FilterMode,
    Format, MemoryType, ResourceBinding, ResourceId, SamplerDesc, TextureDesc, TextureType,
    TextureUsage,
};

struct BufferResource {
    handle: wgpu::Buffer,
    logical_size: u64,
    logical_len: usize,
    memory_type: MemoryType,
    shadow: Option<Vec<u8>>,
    mapped: bool,
}

struct TextureResource {
    handle: wgpu::Texture,
    view: wgpu::TextureView,
}

struct PipelineResource {
    handle: wgpu::ComputePipeline,
}

/// NVIDIA Flow resource and command adapter for a shared `wgpu` device.
///
/// Flow commands are encoded into a dedicated command encoder. The renderer can
/// either take its command buffer and submit it with the rest of the frame, or
/// use [`Self::submit`] for a standalone submission.
pub struct WgpuBackend {
    device: Arc<wgpu::Device>,
    queue: Arc<wgpu::Queue>,
    current_frame: u64,
    last_completed_frame: u64,
    next_id: u64,
    buffers: BTreeMap<ResourceId, BufferResource>,
    textures: BTreeMap<ResourceId, TextureResource>,
    samplers: BTreeMap<ResourceId, wgpu::Sampler>,
    pipelines: BTreeMap<ResourceId, PipelineResource>,
    encoder: Option<wgpu::CommandEncoder>,
}

impl WgpuBackend {
    /// Device features required by Flow's core Grid samplers.
    #[must_use]
    pub const fn required_features() -> wgpu::Features {
        wgpu::Features::ADDRESS_MODE_CLAMP_TO_ZERO
    }

    /// Creates an adapter over renderer-owned device and queue handles.
    #[must_use]
    pub fn new(
        device: Arc<wgpu::Device>,
        queue: Arc<wgpu::Queue>,
        current_frame: u64,
        last_completed_frame: u64,
    ) -> Self {
        Self {
            device,
            queue,
            current_frame,
            last_completed_frame,
            next_id: 0,
            buffers: BTreeMap::new(),
            textures: BTreeMap::new(),
            samplers: BTreeMap::new(),
            pipelines: BTreeMap::new(),
            encoder: None,
        }
    }

    /// Starts a Flow frame and allocates its command encoder.
    pub fn begin_frame(
        &mut self,
        current_frame: u64,
        last_completed_frame: u64,
    ) -> Result<(), BackendError> {
        if self.encoder.is_some() {
            return Err(BackendError::Rejected(
                "begin frame with pending Flow commands",
            ));
        }
        if current_frame < self.current_frame || last_completed_frame < self.last_completed_frame {
            return Err(BackendError::Rejected(
                "Flow frame counters must be monotonic",
            ));
        }
        if last_completed_frame > current_frame {
            return Err(BackendError::Rejected(
                "completed Flow frame is newer than current frame",
            ));
        }
        self.current_frame = current_frame;
        self.last_completed_frame = last_completed_frame;
        self.encoder = Some(
            self.device
                .create_command_encoder(&wgpu::CommandEncoderDescriptor {
                    label: Some("Blackflower NVIDIA Flow"),
                }),
        );
        Ok(())
    }

    /// Finishes and returns the current Flow command buffer for ordered renderer submission.
    pub fn finish_commands(&mut self) -> Option<wgpu::CommandBuffer> {
        self.encoder.take().map(wgpu::CommandEncoder::finish)
    }

    /// Finishes and immediately submits the current Flow command buffer.
    pub fn submit(&mut self) -> Option<wgpu::SubmissionIndex> {
        let commands = self.finish_commands()?;
        Some(self.queue.submit([commands]))
    }

    /// The shared renderer device.
    #[must_use]
    pub fn device(&self) -> &wgpu::Device {
        &self.device
    }

    /// The shared renderer queue.
    #[must_use]
    pub fn queue(&self) -> &wgpu::Queue {
        &self.queue
    }

    fn allocate_id(&mut self) -> Result<ResourceId, BackendError> {
        self.next_id = self
            .next_id
            .checked_add(1)
            .ok_or(BackendError::InvalidResourceId)?;
        ResourceId::new(self.next_id).ok_or(BackendError::InvalidResourceId)
    }

    fn encoder(&mut self) -> Result<&mut wgpu::CommandEncoder, BackendError> {
        self.encoder
            .as_mut()
            .ok_or(BackendError::Rejected("Flow frame has not begun"))
    }
}

impl Backend for WgpuBackend {
    fn current_frame(&self) -> u64 {
        self.current_frame
    }

    fn last_completed_frame(&self) -> u64 {
        self.last_completed_frame
    }

    fn create_buffer(&mut self, desc: BufferDesc) -> Result<ResourceId, BackendError> {
        if desc.size_in_bytes == 0 {
            return Err(BackendError::Rejected("zero-sized Flow buffer"));
        }
        let logical_size = usize::try_from(desc.size_in_bytes)
            .map_err(|_error| BackendError::Rejected("Flow buffer exceeds address space"))?;
        let physical_size = align_copy_size(desc.size_in_bytes)?;
        let physical_len = usize::try_from(physical_size)
            .map_err(|_error| BackendError::Rejected("Flow buffer exceeds address space"))?;
        let usage = buffer_usage(desc)?;
        validate_buffer_limits(desc, physical_size, &self.device.limits())?;
        let shadow = match desc.memory_type {
            MemoryType::Upload | MemoryType::Readback => Some(vec![0; physical_len]),
            MemoryType::Device => None,
            MemoryType::Unknown(_) => {
                return Err(BackendError::Unsupported("unknown Flow buffer memory type"));
            }
        };
        debug_assert!(logical_size <= physical_len);

        let handle = self.device.create_buffer(&wgpu::BufferDescriptor {
            label: Some("NVIDIA Flow buffer"),
            size: physical_size,
            usage,
            mapped_at_creation: false,
        });
        let id = self.allocate_id()?;
        self.buffers.insert(
            id,
            BufferResource {
                handle,
                logical_size: desc.size_in_bytes,
                logical_len: logical_size,
                memory_type: desc.memory_type,
                shadow,
                mapped: false,
            },
        );
        Ok(id)
    }

    fn destroy_buffer(&mut self, buffer: ResourceId) -> Result<(), BackendError> {
        self.buffers
            .remove(&buffer)
            .ok_or(BackendError::Rejected("destroy unknown Flow buffer"))?;
        Ok(())
    }

    fn map_buffer(&mut self, buffer: ResourceId) -> Result<&mut [u8], BackendError> {
        let resource = self
            .buffers
            .get_mut(&buffer)
            .ok_or(BackendError::Rejected("map unknown Flow buffer"))?;
        if resource.mapped {
            return Err(BackendError::Rejected("Flow buffer is already mapped"));
        }

        if resource.memory_type == MemoryType::Readback {
            let (sender, receiver) = mpsc::sync_channel(1);
            resource
                .handle
                .map_async(wgpu::MapMode::Read, .., move |result| {
                    let _result = sender.send(result);
                });
            self.device
                .poll(wgpu::PollType::wait_indefinitely())
                .map_err(|_error| BackendError::Rejected("poll Flow readback buffer"))?;
            receiver
                .recv()
                .map_err(|_error| BackendError::Rejected("receive Flow readback mapping"))?
                .map_err(|_error| BackendError::Rejected("map Flow readback buffer"))?;
            let mapped = resource
                .handle
                .get_mapped_range(..)
                .map_err(|_error| BackendError::Rejected("read Flow readback mapping"))?;
            resource
                .shadow
                .as_mut()
                .ok_or(BackendError::Rejected("missing Flow readback shadow"))?
                .copy_from_slice(&mapped);
            drop(mapped);
            resource.handle.unmap();
        } else if resource.memory_type != MemoryType::Upload {
            return Err(BackendError::Unsupported("map device-local Flow buffer"));
        }

        resource.mapped = true;
        resource
            .shadow
            .as_mut()
            .map(|shadow| &mut shadow[..resource.logical_len])
            .ok_or(BackendError::Rejected("missing Flow buffer shadow"))
    }

    fn unmap_buffer(&mut self, buffer: ResourceId) -> Result<(), BackendError> {
        let resource = self
            .buffers
            .get_mut(&buffer)
            .ok_or(BackendError::Rejected("unmap unknown Flow buffer"))?;
        if !resource.mapped {
            return Err(BackendError::Rejected("Flow buffer is not mapped"));
        }
        if resource.memory_type == MemoryType::Upload {
            self.queue.write_buffer(
                &resource.handle,
                0,
                resource
                    .shadow
                    .as_deref()
                    .ok_or(BackendError::Rejected("missing Flow upload shadow"))?,
            );
        }
        resource.mapped = false;
        Ok(())
    }

    fn create_texture(&mut self, desc: TextureDesc) -> Result<ResourceId, BackendError> {
        let (dimension, size) = texture_shape(desc)?;
        validate_texture_limits(dimension, size, &self.device.limits())?;
        let format = texture_format(desc.format)?;
        let usage = texture_usage(desc.usage)?;
        let handle = self.device.create_texture(&wgpu::TextureDescriptor {
            label: Some("NVIDIA Flow texture"),
            size,
            mip_level_count: desc.mip_levels,
            sample_count: 1,
            dimension,
            format,
            usage,
            view_formats: &[],
        });
        let view = handle.create_view(&wgpu::TextureViewDescriptor::default());
        let id = self.allocate_id()?;
        self.textures.insert(id, TextureResource { handle, view });
        Ok(id)
    }

    fn destroy_texture(&mut self, texture: ResourceId) -> Result<(), BackendError> {
        self.textures
            .remove(&texture)
            .ok_or(BackendError::Rejected("destroy unknown Flow texture"))?;
        Ok(())
    }

    fn create_sampler(&mut self, desc: SamplerDesc) -> Result<ResourceId, BackendError> {
        let address_mode_u = address_mode(desc.address_mode_u, &self.device)?;
        let address_mode_v = address_mode(desc.address_mode_v, &self.device)?;
        let address_mode_w = address_mode(desc.address_mode_w, &self.device)?;
        let border_color = if [
            desc.address_mode_u,
            desc.address_mode_v,
            desc.address_mode_w,
        ]
        .contains(&AddressMode::BorderZero)
        {
            Some(wgpu::SamplerBorderColor::Zero)
        } else {
            None
        };
        let (filter, mipmap_filter) = filter_mode(desc.filter_mode)?;
        let handle = self.device.create_sampler(&wgpu::SamplerDescriptor {
            label: Some("NVIDIA Flow sampler"),
            address_mode_u,
            address_mode_v,
            address_mode_w,
            mag_filter: filter,
            min_filter: filter,
            mipmap_filter,
            border_color,
            ..Default::default()
        });
        let id = self.allocate_id()?;
        self.samplers.insert(id, handle);
        Ok(id)
    }

    fn destroy_sampler(&mut self, sampler: ResourceId) -> Result<(), BackendError> {
        self.samplers
            .remove(&sampler)
            .ok_or(BackendError::Rejected("destroy unknown Flow sampler"))?;
        Ok(())
    }

    fn create_compute_pipeline(
        &mut self,
        desc: ComputePipelineDesc<'_>,
    ) -> Result<ResourceId, BackendError> {
        validate_pipeline_desc(desc)?;
        validate_pipeline_limits(desc, &self.device.limits())?;
        let words = spirv_words(desc.bytecode)?;
        let module = self
            .device
            .create_shader_module(wgpu::ShaderModuleDescriptor {
                label: Some("NVIDIA Flow compute shader"),
                source: wgpu::ShaderSource::SpirV(Cow::Owned(words)),
            });
        let handle = self
            .device
            .create_compute_pipeline(&wgpu::ComputePipelineDescriptor {
                label: Some("NVIDIA Flow compute pipeline"),
                layout: None,
                module: &module,
                entry_point: Some("main"),
                compilation_options: Default::default(),
                cache: None,
            });
        let id = self.allocate_id()?;
        self.pipelines.insert(id, PipelineResource { handle });
        Ok(id)
    }

    fn destroy_compute_pipeline(&mut self, pipeline: ResourceId) -> Result<(), BackendError> {
        self.pipelines
            .remove(&pipeline)
            .ok_or(BackendError::Rejected(
                "destroy unknown Flow compute pipeline",
            ))?;
        Ok(())
    }

    fn add_compute_pass(&mut self, pass: ComputePass<'_>) -> Result<(), BackendError> {
        if pass
            .grid
            .into_iter()
            .any(|dimension| dimension > self.device.limits().max_compute_workgroups_per_dimension)
        {
            return Err(BackendError::Rejected("Flow dispatch exceeds wgpu limits"));
        }
        let pipeline = self
            .pipelines
            .get(&pass.pipeline)
            .ok_or(BackendError::Rejected("compute with unknown Flow pipeline"))?
            .handle
            .clone();
        let bind_groups = create_bind_groups(
            &self.device,
            &pipeline,
            pass.resources,
            &self.buffers,
            &self.textures,
            &self.samplers,
        )?;
        let encoder = self.encoder()?;
        let mut compute = encoder.begin_compute_pass(&wgpu::ComputePassDescriptor {
            label: pass.debug_label,
            timestamp_writes: None,
        });
        compute.set_pipeline(&pipeline);
        for (set, bind_group) in &bind_groups {
            compute.set_bind_group(*set, bind_group, &[]);
        }
        compute.dispatch_workgroups(pass.grid[0], pass.grid[1], pass.grid[2]);
        Ok(())
    }

    fn add_copy_buffer_pass(&mut self, pass: CopyBufferPass<'_>) -> Result<(), BackendError> {
        if pass.size == 0 || !pass.size.is_multiple_of(wgpu::COPY_BUFFER_ALIGNMENT) {
            return Err(BackendError::Rejected("invalid Flow buffer copy size"));
        }
        let source =
            checked_buffer_copy(&self.buffers, pass.source, pass.source_offset, pass.size)?;
        let destination = checked_buffer_copy(
            &self.buffers,
            pass.destination,
            pass.destination_offset,
            pass.size,
        )?;
        let encoder = self.encoder()?;
        insert_debug_marker(encoder, pass.debug_label);
        encoder.copy_buffer_to_buffer(
            &source,
            pass.source_offset,
            &destination,
            pass.destination_offset,
            pass.size,
        );
        Ok(())
    }

    fn add_copy_buffer_to_texture_pass(
        &mut self,
        pass: BufferTextureCopyPass<'_>,
    ) -> Result<(), BackendError> {
        let buffer = self
            .buffers
            .get(&pass.buffer)
            .ok_or(BackendError::Rejected("copy from unknown Flow buffer"))?
            .handle
            .clone();
        let texture = self
            .textures
            .get(&pass.texture)
            .ok_or(BackendError::Rejected("copy to unknown Flow texture"))?
            .handle
            .clone();
        let layout = texture_copy_layout(pass)?;
        let encoder = self.encoder()?;
        insert_debug_marker(encoder, pass.debug_label);
        encoder.copy_buffer_to_texture(
            wgpu::TexelCopyBufferInfo {
                buffer: &buffer,
                layout,
            },
            texture_copy_info(&texture, pass.mip_level, pass.offset),
            extent(pass.extent)?,
        );
        Ok(())
    }

    fn add_copy_texture_to_buffer_pass(
        &mut self,
        pass: BufferTextureCopyPass<'_>,
    ) -> Result<(), BackendError> {
        let buffer = self
            .buffers
            .get(&pass.buffer)
            .ok_or(BackendError::Rejected("copy to unknown Flow buffer"))?
            .handle
            .clone();
        let texture = self
            .textures
            .get(&pass.texture)
            .ok_or(BackendError::Rejected("copy from unknown Flow texture"))?
            .handle
            .clone();
        let layout = texture_copy_layout(pass)?;
        let encoder = self.encoder()?;
        insert_debug_marker(encoder, pass.debug_label);
        encoder.copy_texture_to_buffer(
            texture_copy_info(&texture, pass.mip_level, pass.offset),
            wgpu::TexelCopyBufferInfo {
                buffer: &buffer,
                layout,
            },
            extent(pass.extent)?,
        );
        Ok(())
    }

    fn add_copy_texture_pass(&mut self, pass: CopyTexturePass<'_>) -> Result<(), BackendError> {
        let source = self
            .textures
            .get(&pass.source)
            .ok_or(BackendError::Rejected("copy from unknown Flow texture"))?
            .handle
            .clone();
        let destination = self
            .textures
            .get(&pass.destination)
            .ok_or(BackendError::Rejected("copy to unknown Flow texture"))?
            .handle
            .clone();
        let encoder = self.encoder()?;
        insert_debug_marker(encoder, pass.debug_label);
        encoder.copy_texture_to_texture(
            texture_copy_info(&source, pass.source_mip_level, pass.source_offset),
            texture_copy_info(
                &destination,
                pass.destination_mip_level,
                pass.destination_offset,
            ),
            extent(pass.extent)?,
        );
        Ok(())
    }
}

fn align_copy_size(size: u64) -> Result<u64, BackendError> {
    let alignment = wgpu::COPY_BUFFER_ALIGNMENT;
    size.checked_add(alignment - 1)
        .map(|value| value / alignment * alignment)
        .ok_or(BackendError::Rejected("Flow buffer size overflow"))
}

fn buffer_usage(desc: BufferDesc) -> Result<wgpu::BufferUsages, BackendError> {
    let mut usage = wgpu::BufferUsages::empty();
    if desc.usage.contains(BufferUsage::CONSTANT) {
        usage |= wgpu::BufferUsages::UNIFORM;
    }
    if desc.usage.contains(BufferUsage::STRUCTURED)
        || desc.usage.contains(BufferUsage::RAW)
        || desc.usage.contains(BufferUsage::STORAGE_STRUCTURED)
        || desc.usage.contains(BufferUsage::STORAGE_RAW)
    {
        usage |= wgpu::BufferUsages::STORAGE;
    }
    if desc.usage.contains(BufferUsage::INDIRECT) {
        usage |= wgpu::BufferUsages::INDIRECT;
    }
    if desc.usage.contains(BufferUsage::COPY_SOURCE) {
        usage |= wgpu::BufferUsages::COPY_SRC;
    }
    if desc.usage.contains(BufferUsage::COPY_DESTINATION) {
        usage |= wgpu::BufferUsages::COPY_DST;
    }

    match desc.memory_type {
        MemoryType::Device => {}
        MemoryType::Upload => usage |= wgpu::BufferUsages::COPY_DST,
        MemoryType::Readback => {
            if !usage.difference(wgpu::BufferUsages::COPY_DST).is_empty() {
                return Err(BackendError::Unsupported(
                    "readback Flow buffer usage beyond copy destination",
                ));
            }
            usage = wgpu::BufferUsages::MAP_READ | wgpu::BufferUsages::COPY_DST;
        }
        MemoryType::Unknown(_) => {
            return Err(BackendError::Unsupported("unknown Flow buffer memory type"));
        }
    }
    if usage.is_empty() {
        return Err(BackendError::Rejected("Flow buffer has no usage"));
    }
    Ok(usage)
}

fn validate_buffer_limits(
    desc: BufferDesc,
    physical_size: u64,
    limits: &wgpu::Limits,
) -> Result<(), BackendError> {
    if physical_size > limits.max_buffer_size {
        return Err(BackendError::Rejected(
            "Flow buffer exceeds wgpu size limit",
        ));
    }
    if desc.usage.contains(BufferUsage::CONSTANT)
        && desc.size_in_bytes > limits.max_uniform_buffer_binding_size
    {
        return Err(BackendError::Rejected(
            "Flow constant buffer exceeds wgpu binding limit",
        ));
    }
    if (desc.usage.contains(BufferUsage::STRUCTURED)
        || desc.usage.contains(BufferUsage::RAW)
        || desc.usage.contains(BufferUsage::STORAGE_STRUCTURED)
        || desc.usage.contains(BufferUsage::STORAGE_RAW))
        && desc.size_in_bytes > limits.max_storage_buffer_binding_size
    {
        return Err(BackendError::Rejected(
            "Flow storage buffer exceeds wgpu binding limit",
        ));
    }
    Ok(())
}

fn texture_usage(usage: TextureUsage) -> Result<wgpu::TextureUsages, BackendError> {
    let mut mapped = wgpu::TextureUsages::empty();
    if usage.contains(TextureUsage::SAMPLED) {
        mapped |= wgpu::TextureUsages::TEXTURE_BINDING;
    }
    if usage.contains(TextureUsage::STORAGE) {
        mapped |= wgpu::TextureUsages::STORAGE_BINDING;
    }
    if usage.contains(TextureUsage::COPY_SOURCE) {
        mapped |= wgpu::TextureUsages::COPY_SRC;
    }
    if usage.contains(TextureUsage::COPY_DESTINATION) {
        mapped |= wgpu::TextureUsages::COPY_DST;
    }
    if mapped.is_empty() {
        Err(BackendError::Rejected("Flow texture has no usage"))
    } else {
        Ok(mapped)
    }
}

fn texture_shape(
    desc: TextureDesc,
) -> Result<(wgpu::TextureDimension, wgpu::Extent3d), BackendError> {
    if desc.width == 0 || desc.height == 0 || desc.depth == 0 || desc.mip_levels == 0 {
        return Err(BackendError::Rejected("invalid Flow texture extent"));
    }
    let dimension = match desc.texture_type {
        TextureType::OneDimensional if desc.height == 1 && desc.depth == 1 => {
            wgpu::TextureDimension::D1
        }
        TextureType::TwoDimensional if desc.depth == 1 => wgpu::TextureDimension::D2,
        TextureType::ThreeDimensional => wgpu::TextureDimension::D3,
        TextureType::OneDimensional | TextureType::TwoDimensional => {
            return Err(BackendError::Rejected(
                "invalid Flow texture dimensionality",
            ));
        }
        TextureType::Unknown(_) => {
            return Err(BackendError::Unsupported("unknown Flow texture type"));
        }
    };
    Ok((
        dimension,
        wgpu::Extent3d {
            width: desc.width,
            height: desc.height,
            depth_or_array_layers: desc.depth,
        },
    ))
}

fn validate_texture_limits(
    dimension: wgpu::TextureDimension,
    size: wgpu::Extent3d,
    limits: &wgpu::Limits,
) -> Result<(), BackendError> {
    let valid = match dimension {
        wgpu::TextureDimension::D1 => size.width <= limits.max_texture_dimension_1d,
        wgpu::TextureDimension::D2 => {
            size.width <= limits.max_texture_dimension_2d
                && size.height <= limits.max_texture_dimension_2d
        }
        wgpu::TextureDimension::D3 => {
            size.width <= limits.max_texture_dimension_3d
                && size.height <= limits.max_texture_dimension_3d
                && size.depth_or_array_layers <= limits.max_texture_dimension_3d
        }
    };
    if valid {
        Ok(())
    } else {
        Err(BackendError::Rejected(
            "Flow texture exceeds wgpu dimension limit",
        ))
    }
}

fn texture_format(format: Format) -> Result<wgpu::TextureFormat, BackendError> {
    match format {
        Format::RGBA32_FLOAT => Ok(wgpu::TextureFormat::Rgba32Float),
        Format::RGBA32_UINT => Ok(wgpu::TextureFormat::Rgba32Uint),
        Format::RGBA32_SINT => Ok(wgpu::TextureFormat::Rgba32Sint),
        Format::RGB32_FLOAT | Format::RGB32_UINT | Format::RGB32_SINT => Err(
            BackendError::Unsupported("three-channel 32-bit Flow texture format"),
        ),
        Format::RGBA16_FLOAT => Ok(wgpu::TextureFormat::Rgba16Float),
        Format::RGBA16_UNORM => Ok(wgpu::TextureFormat::Rgba16Unorm),
        Format::RGBA16_UINT => Ok(wgpu::TextureFormat::Rgba16Uint),
        Format::RGBA16_SNORM => Ok(wgpu::TextureFormat::Rgba16Snorm),
        Format::RGBA16_SINT => Ok(wgpu::TextureFormat::Rgba16Sint),
        Format::RG32_FLOAT => Ok(wgpu::TextureFormat::Rg32Float),
        Format::RG32_UINT => Ok(wgpu::TextureFormat::Rg32Uint),
        Format::RG32_SINT => Ok(wgpu::TextureFormat::Rg32Sint),
        Format::RGB10A2_UNORM => Ok(wgpu::TextureFormat::Rgb10a2Unorm),
        Format::RGB10A2_UINT => Ok(wgpu::TextureFormat::Rgb10a2Uint),
        Format::RG11B10_FLOAT => Ok(wgpu::TextureFormat::Rg11b10Ufloat),
        Format::RGBA8_UNORM => Ok(wgpu::TextureFormat::Rgba8Unorm),
        Format::RGBA8_UNORM_SRGB => Ok(wgpu::TextureFormat::Rgba8UnormSrgb),
        Format::RGBA8_UINT => Ok(wgpu::TextureFormat::Rgba8Uint),
        Format::RGBA8_SNORM => Ok(wgpu::TextureFormat::Rgba8Snorm),
        Format::RGBA8_SINT => Ok(wgpu::TextureFormat::Rgba8Sint),
        Format::RG16_FLOAT => Ok(wgpu::TextureFormat::Rg16Float),
        Format::RG16_UNORM => Ok(wgpu::TextureFormat::Rg16Unorm),
        Format::RG16_UINT => Ok(wgpu::TextureFormat::Rg16Uint),
        Format::RG16_SNORM => Ok(wgpu::TextureFormat::Rg16Snorm),
        Format::RG16_SINT => Ok(wgpu::TextureFormat::Rg16Sint),
        Format::R32_FLOAT => Ok(wgpu::TextureFormat::R32Float),
        Format::R32_UINT => Ok(wgpu::TextureFormat::R32Uint),
        Format::R32_SINT => Ok(wgpu::TextureFormat::R32Sint),
        Format::RG8_UNORM => Ok(wgpu::TextureFormat::Rg8Unorm),
        Format::RG8_UINT => Ok(wgpu::TextureFormat::Rg8Uint),
        Format::RG8_SNORM => Ok(wgpu::TextureFormat::Rg8Snorm),
        Format::RG8_SINT => Ok(wgpu::TextureFormat::Rg8Sint),
        Format::R16_FLOAT => Ok(wgpu::TextureFormat::R16Float),
        Format::R16_UNORM => Ok(wgpu::TextureFormat::R16Unorm),
        Format::R16_UINT => Ok(wgpu::TextureFormat::R16Uint),
        Format::R16_SNORM => Ok(wgpu::TextureFormat::R16Snorm),
        Format::R16_SINT => Ok(wgpu::TextureFormat::R16Sint),
        Format::R8_UNORM => Ok(wgpu::TextureFormat::R8Unorm),
        Format::R8_UINT => Ok(wgpu::TextureFormat::R8Uint),
        Format::R8_SNORM => Ok(wgpu::TextureFormat::R8Snorm),
        Format::R8_SINT => Ok(wgpu::TextureFormat::R8Sint),
        Format::BGRA8_UNORM => Ok(wgpu::TextureFormat::Bgra8Unorm),
        Format::BGRA8_UNORM_SRGB => Ok(wgpu::TextureFormat::Bgra8UnormSrgb),
        _ => Err(BackendError::Unsupported("unknown Flow texture format")),
    }
}

fn address_mode(
    mode: AddressMode,
    device: &wgpu::Device,
) -> Result<wgpu::AddressMode, BackendError> {
    match mode {
        AddressMode::Wrap => Ok(wgpu::AddressMode::Repeat),
        AddressMode::Clamp => Ok(wgpu::AddressMode::ClampToEdge),
        AddressMode::Mirror => Ok(wgpu::AddressMode::MirrorRepeat),
        AddressMode::BorderZero
            if device
                .features()
                .contains(wgpu::Features::ADDRESS_MODE_CLAMP_TO_ZERO) =>
        {
            Ok(wgpu::AddressMode::ClampToBorder)
        }
        AddressMode::BorderZero => Err(BackendError::Unsupported(
            "Flow zero-border sampler without wgpu feature",
        )),
        AddressMode::Unknown(_) => Err(BackendError::Unsupported("unknown Flow address mode")),
    }
}

fn filter_mode(
    mode: FilterMode,
) -> Result<(wgpu::FilterMode, wgpu::MipmapFilterMode), BackendError> {
    match mode {
        FilterMode::Point => Ok((wgpu::FilterMode::Nearest, wgpu::MipmapFilterMode::Nearest)),
        FilterMode::Linear => Ok((wgpu::FilterMode::Linear, wgpu::MipmapFilterMode::Linear)),
        FilterMode::Unknown(_) => Err(BackendError::Unsupported("unknown Flow filter mode")),
    }
}

fn validate_pipeline_desc(desc: ComputePipelineDesc<'_>) -> Result<(), BackendError> {
    if desc.bytecode.is_empty() || !desc.bytecode.len().is_multiple_of(size_of::<u32>()) {
        return Err(BackendError::Rejected("invalid Flow SPIR-V bytecode"));
    }
    let mut locations = BTreeSet::new();
    for binding in desc.bindings {
        if binding.descriptor_count != 1 {
            return Err(BackendError::Unsupported("Flow descriptor arrays"));
        }
        if !locations.insert((binding.set, binding.binding)) {
            return Err(BackendError::Rejected("duplicate Flow pipeline binding"));
        }
        match binding.descriptor_type {
            DescriptorType::CONSTANT_BUFFER
            | DescriptorType::STRUCTURED_BUFFER
            | DescriptorType::TEXTURE
            | DescriptorType::SAMPLER
            | DescriptorType::RW_STRUCTURED_BUFFER
            | DescriptorType::RW_TEXTURE => {}
            DescriptorType::BUFFER | DescriptorType::RW_BUFFER => {
                return Err(BackendError::Unsupported("Flow texel buffers"));
            }
            DescriptorType::TEXTURE_SAMPLER => {
                return Err(BackendError::Unsupported("combined Flow texture sampler"));
            }
            _ => {
                return Err(BackendError::Rejected(
                    "invalid Flow shader descriptor type",
                ));
            }
        }
    }
    Ok(())
}

fn validate_pipeline_limits(
    desc: ComputePipelineDesc<'_>,
    limits: &wgpu::Limits,
) -> Result<(), BackendError> {
    let mut uniform_buffers = 0_u32;
    let mut storage_buffers = 0_u32;
    let mut sampled_textures = 0_u32;
    let mut samplers = 0_u32;
    let mut storage_textures = 0_u32;
    for binding in desc.bindings {
        if binding.set >= limits.max_bind_groups
            || binding.binding > limits.max_bindings_per_bind_group
        {
            return Err(BackendError::Rejected(
                "Flow binding location exceeds wgpu limits",
            ));
        }
        match binding.descriptor_type {
            DescriptorType::CONSTANT_BUFFER => uniform_buffers += 1,
            DescriptorType::STRUCTURED_BUFFER | DescriptorType::RW_STRUCTURED_BUFFER => {
                storage_buffers += 1;
            }
            DescriptorType::TEXTURE => sampled_textures += 1,
            DescriptorType::SAMPLER => samplers += 1,
            DescriptorType::RW_TEXTURE => storage_textures += 1,
            _ => {}
        }
    }
    if uniform_buffers > limits.max_uniform_buffers_per_shader_stage
        || storage_buffers > limits.max_storage_buffers_per_shader_stage
        || sampled_textures > limits.max_sampled_textures_per_shader_stage
        || samplers > limits.max_samplers_per_shader_stage
        || storage_textures > limits.max_storage_textures_per_shader_stage
    {
        return Err(BackendError::Rejected(
            "Flow shader resources exceed wgpu limits",
        ));
    }
    Ok(())
}

fn spirv_words(bytecode: &[u8]) -> Result<Vec<u32>, BackendError> {
    let words = bytecode
        .chunks_exact(size_of::<u32>())
        .map(|bytes| u32::from_le_bytes([bytes[0], bytes[1], bytes[2], bytes[3]]))
        .collect::<Vec<_>>();
    if words.first().copied() != Some(0x0723_0203) {
        return Err(BackendError::Rejected("invalid Flow SPIR-V magic"));
    }
    Ok(words)
}

fn create_bind_groups(
    device: &wgpu::Device,
    pipeline: &wgpu::ComputePipeline,
    resources: &[ResourceBinding],
    buffers: &BTreeMap<ResourceId, BufferResource>,
    textures: &BTreeMap<ResourceId, TextureResource>,
    samplers: &BTreeMap<ResourceId, wgpu::Sampler>,
) -> Result<Vec<(u32, wgpu::BindGroup)>, BackendError> {
    let mut grouped = BTreeMap::<u32, Vec<&ResourceBinding>>::new();
    let mut locations = BTreeSet::new();
    for resource in resources {
        if resource.array_index != 0 {
            return Err(BackendError::Unsupported("Flow descriptor arrays"));
        }
        if !locations.insert((resource.set, resource.binding)) {
            return Err(BackendError::Rejected("duplicate Flow resource binding"));
        }
        grouped.entry(resource.set).or_default().push(resource);
    }

    let mut bind_groups = Vec::with_capacity(grouped.len());
    for (set, resources) in grouped {
        let entries = resources
            .iter()
            .map(|resource| bind_group_entry(resource, buffers, textures, samplers))
            .collect::<Result<Vec<_>, _>>()?;
        let layout = pipeline.get_bind_group_layout(set);
        let bind_group = device.create_bind_group(&wgpu::BindGroupDescriptor {
            label: Some("NVIDIA Flow resources"),
            layout: &layout,
            entries: &entries,
        });
        bind_groups.push((set, bind_group));
    }
    Ok(bind_groups)
}

fn bind_group_entry<'a>(
    resource: &ResourceBinding,
    buffers: &'a BTreeMap<ResourceId, BufferResource>,
    textures: &'a BTreeMap<ResourceId, TextureResource>,
    samplers: &'a BTreeMap<ResourceId, wgpu::Sampler>,
) -> Result<wgpu::BindGroupEntry<'a>, BackendError> {
    let binding_resource = match resource.descriptor_type {
        DescriptorType::CONSTANT_BUFFER
        | DescriptorType::STRUCTURED_BUFFER
        | DescriptorType::RW_STRUCTURED_BUFFER => {
            let id = only_buffer(resource)?;
            buffers
                .get(&id)
                .ok_or(BackendError::Rejected("bind unknown Flow buffer"))?
                .handle
                .as_entire_binding()
        }
        DescriptorType::TEXTURE | DescriptorType::RW_TEXTURE => {
            let id = only_texture(resource)?;
            wgpu::BindingResource::TextureView(
                &textures
                    .get(&id)
                    .ok_or(BackendError::Rejected("bind unknown Flow texture"))?
                    .view,
            )
        }
        DescriptorType::SAMPLER => {
            let id = only_sampler(resource)?;
            wgpu::BindingResource::Sampler(
                samplers
                    .get(&id)
                    .ok_or(BackendError::Rejected("bind unknown Flow sampler"))?,
            )
        }
        DescriptorType::BUFFER | DescriptorType::RW_BUFFER => {
            return Err(BackendError::Unsupported("Flow texel buffers"));
        }
        DescriptorType::TEXTURE_SAMPLER => {
            return Err(BackendError::Unsupported("combined Flow texture sampler"));
        }
        _ => {
            return Err(BackendError::Rejected(
                "invalid Flow resource descriptor type",
            ));
        }
    };
    Ok(wgpu::BindGroupEntry {
        binding: resource.binding,
        resource: binding_resource,
    })
}

fn only_buffer(resource: &ResourceBinding) -> Result<ResourceId, BackendError> {
    match (resource.buffer, resource.texture, resource.sampler) {
        (Some(buffer), None, None) => Ok(buffer),
        _ => Err(BackendError::Rejected("invalid Flow buffer binding")),
    }
}

fn only_texture(resource: &ResourceBinding) -> Result<ResourceId, BackendError> {
    match (resource.buffer, resource.texture, resource.sampler) {
        (None, Some(texture), None) => Ok(texture),
        _ => Err(BackendError::Rejected("invalid Flow texture binding")),
    }
}

fn only_sampler(resource: &ResourceBinding) -> Result<ResourceId, BackendError> {
    match (resource.buffer, resource.texture, resource.sampler) {
        (None, None, Some(sampler)) => Ok(sampler),
        _ => Err(BackendError::Rejected("invalid Flow sampler binding")),
    }
}

fn checked_buffer_copy(
    buffers: &BTreeMap<ResourceId, BufferResource>,
    id: ResourceId,
    offset: u64,
    size: u64,
) -> Result<wgpu::Buffer, BackendError> {
    let resource = buffers
        .get(&id)
        .ok_or(BackendError::Rejected("copy unknown Flow buffer"))?;
    if offset
        .checked_add(size)
        .is_none_or(|end| end > resource.logical_size)
    {
        return Err(BackendError::Rejected("Flow buffer copy is out of bounds"));
    }
    Ok(resource.handle.clone())
}

fn texture_copy_layout(
    pass: BufferTextureCopyPass<'_>,
) -> Result<wgpu::TexelCopyBufferLayout, BackendError> {
    let multiple_rows = pass.extent[1] > 1 || pass.extent[2] > 1;
    let bytes_per_row = if multiple_rows {
        if pass.buffer_row_pitch == 0 {
            return Err(BackendError::Rejected("missing Flow texture row pitch"));
        }
        if !pass
            .buffer_row_pitch
            .is_multiple_of(wgpu::COPY_BYTES_PER_ROW_ALIGNMENT)
        {
            return Err(BackendError::Rejected("unaligned Flow texture row pitch"));
        }
        Some(pass.buffer_row_pitch)
    } else {
        None
    };
    let rows_per_image = if pass.extent[2] > 1 {
        if pass.buffer_row_pitch == 0
            || pass.buffer_depth_pitch == 0
            || !pass
                .buffer_depth_pitch
                .is_multiple_of(pass.buffer_row_pitch)
        {
            return Err(BackendError::Rejected("invalid Flow texture depth pitch"));
        }
        let rows = pass.buffer_depth_pitch / pass.buffer_row_pitch;
        if rows < pass.extent[1] {
            return Err(BackendError::Rejected("short Flow texture depth pitch"));
        }
        Some(rows)
    } else {
        None
    };
    Ok(wgpu::TexelCopyBufferLayout {
        offset: pass.buffer_offset,
        bytes_per_row,
        rows_per_image,
    })
}

fn texture_copy_info(
    texture: &wgpu::Texture,
    mip_level: u32,
    offset: [u32; 3],
) -> wgpu::TexelCopyTextureInfo<'_> {
    wgpu::TexelCopyTextureInfo {
        texture,
        mip_level,
        origin: wgpu::Origin3d {
            x: offset[0],
            y: offset[1],
            z: offset[2],
        },
        aspect: wgpu::TextureAspect::All,
    }
}

fn extent(value: [u32; 3]) -> Result<wgpu::Extent3d, BackendError> {
    if value.contains(&0) {
        return Err(BackendError::Rejected("empty Flow texture copy extent"));
    }
    Ok(wgpu::Extent3d {
        width: value[0],
        height: value[1],
        depth_or_array_layers: value[2],
    })
}

fn insert_debug_marker(encoder: &mut wgpu::CommandEncoder, label: Option<&str>) {
    if let Some(label) = label {
        encoder.insert_debug_marker(label);
    }
}

#[cfg(test)]
#[path = "../tests/unit/wgpu_backend.rs"]
mod tests;
