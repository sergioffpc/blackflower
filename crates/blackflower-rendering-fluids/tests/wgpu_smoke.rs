use std::future::Future;
use std::sync::Arc;
use std::task::{Context as TaskContext, Poll, Wake, Waker};
use std::thread;

use blackflower_rendering_fluids::{
    AddressMode, Backend, BackendError, BufferDesc, BufferUsage, CopyBufferPass, FilterMode,
    Format, MemoryType, SamplerDesc, TextureDesc, TextureType, TextureUsage, WgpuBackend,
};

struct ThreadWake(thread::Thread);

impl Wake for ThreadWake {
    fn wake(self: Arc<Self>) {
        self.0.unpark();
    }
}

fn block_on<F: Future>(future: F) -> F::Output {
    let waker = Waker::from(Arc::new(ThreadWake(thread::current())));
    let mut context = TaskContext::from_waker(&waker);
    let mut future = Box::pin(future);
    loop {
        match future.as_mut().poll(&mut context) {
            Poll::Ready(output) => return output,
            Poll::Pending => thread::park(),
        }
    }
}

#[test]
#[ignore = "requires a native GPU adapter"]
fn native_adapter_copies_flow_upload_to_readback() -> Result<(), Box<dyn std::error::Error>> {
    let (device, queue) = native_device()?;
    exercise_resources(device, queue)
}

fn native_device() -> Result<(wgpu::Device, wgpu::Queue), Box<dyn std::error::Error>> {
    let instance = wgpu::Instance::new(wgpu::InstanceDescriptor::new_without_display_handle());
    let adapter = block_on(instance.request_adapter(&wgpu::RequestAdapterOptions::default()))?;
    assert!(
        adapter
            .features()
            .contains(WgpuBackend::required_features()),
        "adapter lacks the zero-border sampler feature required by Flow"
    );
    block_on(adapter.request_device(&wgpu::DeviceDescriptor {
        label: Some("Blackflower Flow smoke test"),
        required_features: WgpuBackend::required_features(),
        required_limits: wgpu::Limits::default(),
        ..Default::default()
    }))
    .map_err(Into::into)
}

fn exercise_resources(
    device: wgpu::Device,
    queue: wgpu::Queue,
) -> Result<(), Box<dyn std::error::Error>> {
    let device = Arc::new(device);
    let queue = Arc::new(queue);
    let mut backend = WgpuBackend::new(Arc::clone(&device), queue, 0, 0);
    exercise_buffer_copy(&mut backend, &device)?;
    exercise_texture_and_sampler(&mut backend)?;
    Ok(())
}

fn exercise_buffer_copy(
    backend: &mut WgpuBackend,
    device: &wgpu::Device,
) -> Result<(), Box<dyn std::error::Error>> {
    let upload = backend.create_buffer(BufferDesc {
        usage: BufferUsage::COPY_SOURCE,
        format: Format::UNKNOWN,
        structure_stride: 0,
        size_in_bytes: 4,
        memory_type: MemoryType::Upload,
    })?;
    backend.map_buffer(upload)?.copy_from_slice(&[1, 2, 3, 4]);
    backend.unmap_buffer(upload)?;

    let readback = backend.create_buffer(BufferDesc {
        usage: BufferUsage::COPY_DESTINATION,
        format: Format::UNKNOWN,
        structure_stride: 0,
        size_in_bytes: 4,
        memory_type: MemoryType::Readback,
    })?;
    backend.begin_frame(1, 0)?;
    backend.add_copy_buffer_pass(CopyBufferPass {
        source: upload,
        destination: readback,
        source_offset: 0,
        destination_offset: 0,
        size: 4,
        debug_label: Some("Flow smoke copy"),
    })?;
    let submission = backend
        .submit()
        .ok_or(BackendError::Rejected("missing Flow smoke commands"))?;
    device.poll(wgpu::PollType::Wait {
        submission_index: Some(submission),
        timeout: None,
    })?;
    assert_eq!(backend.map_buffer(readback)?, &[1, 2, 3, 4]);
    backend.unmap_buffer(readback)?;
    backend.destroy_buffer(readback)?;
    backend.destroy_buffer(upload)?;
    Ok(())
}

fn exercise_texture_and_sampler(
    backend: &mut WgpuBackend,
) -> Result<(), Box<dyn std::error::Error>> {
    let texture = backend.create_texture(TextureDesc {
        texture_type: TextureType::ThreeDimensional,
        usage: TextureUsage::SAMPLED | TextureUsage::STORAGE,
        format: Format::R32_FLOAT,
        width: 4,
        height: 4,
        depth: 4,
        mip_levels: 1,
        optimized_clear_value: [0.0; 4],
    })?;
    let sampler = backend.create_sampler(SamplerDesc {
        address_mode_u: AddressMode::BorderZero,
        address_mode_v: AddressMode::BorderZero,
        address_mode_w: AddressMode::BorderZero,
        filter_mode: FilterMode::Linear,
    })?;
    backend.destroy_sampler(sampler)?;
    backend.destroy_texture(texture)?;
    Ok(())
}
