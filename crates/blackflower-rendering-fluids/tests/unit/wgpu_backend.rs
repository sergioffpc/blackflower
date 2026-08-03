use super::*;

#[test]
fn wgpu_backend_implements_flow_backend_contract() {
    fn assert_backend<T: Backend>() {}
    assert_backend::<WgpuBackend>();
}

#[test]
fn maps_core_grid_texture_formats() {
    assert_eq!(
        texture_format(Format::R32_FLOAT),
        Ok(wgpu::TextureFormat::R32Float)
    );
    assert_eq!(
        texture_format(Format::RGBA16_FLOAT),
        Ok(wgpu::TextureFormat::Rgba16Float)
    );
    assert_eq!(
        texture_format(Format::RGB32_FLOAT),
        Err(BackendError::Unsupported(
            "three-channel 32-bit Flow texture format"
        ))
    );
}

#[test]
fn validates_spirv_before_device_creation() {
    assert_eq!(
        spirv_words(&0x0723_0203_u32.to_le_bytes()),
        Ok(vec![0x0723_0203])
    );
    assert_eq!(
        spirv_words(&[0, 0, 0, 0]),
        Err(BackendError::Rejected("invalid Flow SPIR-V magic"))
    );
}

#[test]
fn rejects_flow_descriptor_shapes_missing_from_webgpu() {
    let binding = crate::BindingDesc {
        descriptor_type: DescriptorType::BUFFER,
        binding: 0,
        descriptor_count: 1,
        set: 0,
    };
    assert_eq!(
        validate_pipeline_desc(ComputePipelineDesc {
            bindings: &[binding],
            bytecode: &0x0723_0203_u32.to_le_bytes(),
        }),
        Err(BackendError::Unsupported("Flow texel buffers"))
    );
}

#[test]
fn reports_the_grid_sampler_feature() {
    assert_eq!(
        WgpuBackend::required_features(),
        wgpu::Features::ADDRESS_MODE_CLAMP_TO_ZERO
    );
}

#[test]
fn converts_single_and_multi_row_copy_layouts() -> Result<(), BackendError> {
    let buffer = ResourceId::new(1).ok_or(BackendError::InvalidResourceId)?;
    let texture = ResourceId::new(2).ok_or(BackendError::InvalidResourceId)?;
    let single_row = texture_copy_layout(BufferTextureCopyPass {
        buffer,
        texture,
        buffer_offset: 0,
        buffer_row_pitch: 12,
        buffer_depth_pitch: 12,
        mip_level: 0,
        offset: [0; 3],
        extent: [3, 1, 1],
        debug_label: None,
    })?;
    assert_eq!(single_row.bytes_per_row, None);

    let multi_row = texture_copy_layout(BufferTextureCopyPass {
        buffer,
        texture,
        buffer_offset: 0,
        buffer_row_pitch: 256,
        buffer_depth_pitch: 1024,
        mip_level: 0,
        offset: [0; 3],
        extent: [16, 4, 2],
        debug_label: None,
    })?;
    assert_eq!(multi_row.bytes_per_row, Some(256));
    assert_eq!(multi_row.rows_per_image, Some(4));
    Ok(())
}
