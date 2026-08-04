#![allow(
    unsafe_code,
    unsafe_op_in_unsafe_fn,
    reason = "all raw Flow calls, callbacks, and native pointer materialization are isolated in this private module"
)]
use std::collections::BTreeMap;
use std::ffi::{CStr, c_void};
use std::marker::PhantomData;
use std::panic::{AssertUnwindSafe, catch_unwind};
use std::ptr::NonNull;
use std::rc::Rc;
use std::slice;

use crate::{
    AddressMode, Backend, BackendError, BindingDesc, BufferDesc, BufferTextureCopyPass,
    BufferUsage, ComputePass, ComputePipelineDesc, CopyBufferPass, CopyTexturePass, DescriptorType,
    Error, Feature, FilterMode, Format, MemoryType, ResourceBinding, ResourceId, SamplerDesc,
    TextureDesc, TextureType, TextureUsage,
};

#[allow(
    dead_code,
    non_camel_case_types,
    non_snake_case,
    non_upper_case_globals,
    unsafe_code,
    reason = "generated declarations mirror the Blackflower Flow C API"
)]
#[allow(
    clippy::all,
    clippy::cast_possible_truncation,
    clippy::cast_possible_wrap,
    clippy::ptr_offset_with_cast,
    clippy::upper_case_acronyms,
    clippy::useless_transmute,
    reason = "bindgen-generated code mirrors C layouts and is not maintained by hand"
)]
#[allow(
    clippy::multiple_unsafe_ops_per_block,
    clippy::undocumented_unsafe_blocks,
    reason = "bindgen output is generated from the pinned Flow wrapper headers"
)]
mod raw {
    include!(concat!(env!("OUT_DIR"), "/flow_bindings.rs"));
}

struct State<B: Backend> {
    backend: B,
    first_error: Option<BackendError>,
    buffer_sizes: BTreeMap<ResourceId, usize>,
}

impl<B: Backend> State<B> {
    fn record(&mut self, error: BackendError) {
        if self.first_error.is_none() {
            self.first_error = Some(error);
        }
    }

    fn reset_error(&mut self) {
        self.first_error = None;
    }

    fn take_error(&mut self) -> Result<(), Error> {
        self.first_error
            .take()
            .map_or(Ok(()), |error| Err(error.into()))
    }
}

pub(crate) struct NativeContext<B: Backend> {
    pointer: NonNull<raw::BFFlowContext>,
    state: Box<State<B>>,
    not_send_or_sync: PhantomData<Rc<()>>,
}

impl<B: Backend> NativeContext<B> {
    pub(crate) fn new(backend: B) -> Result<Self, Error> {
        let mut state = Box::new(State {
            backend,
            first_error: None,
            buffer_sizes: BTreeMap::new(),
        });
        let callbacks = callbacks::<B>((&mut *state as *mut State<B>).cast());
        let mut pointer = std::ptr::null_mut();
        // SAFETY: callbacks remain valid for the context lifetime, their userdata
        // points to the stable boxed state, and `pointer` is uniquely writable.
        check(unsafe { raw::bf_flow_context_create(&callbacks, &mut pointer) })?;
        let pointer = NonNull::new(pointer).ok_or(Error::NativeContract)?;
        if let Err(error) = state.take_error() {
            // SAFETY: the newly created context is uniquely owned and no callback
            // can run after its matching destructor returns.
            unsafe { raw::bf_flow_context_destroy(pointer.as_ptr()) };
            return Err(error);
        }
        Ok(Self {
            pointer,
            state,
            not_send_or_sync: PhantomData,
        })
    }

    pub(crate) fn set_min_resource_lifetime(&mut self, frames: u64) -> Result<(), Error> {
        self.state.reset_error();
        // SAFETY: the context is live and exclusively borrowed; the wrapper
        // accepts the lifetime as a by-value frame count.
        check(unsafe {
            raw::bf_flow_context_set_min_resource_lifetime(self.pointer.as_ptr(), frames)
        })?;
        self.state.take_error()
    }

    pub(crate) fn flush(&mut self) -> Result<(), Error> {
        self.state.reset_error();
        // SAFETY: the context is live and exclusively borrowed while the
        // synchronous flush may invoke its registered callbacks.
        let status = unsafe { raw::bf_flow_context_flush(self.pointer.as_ptr()) };
        self.state.take_error()?;
        check(status)
    }

    pub(crate) fn validate_upload(&mut self, size_in_bytes: u64) -> Result<(), Error> {
        self.state.reset_error();
        // SAFETY: the context is live and exclusively borrowed while the
        // synchronous validation may invoke its registered callbacks.
        let status =
            unsafe { raw::bf_flow_context_validate_upload(self.pointer.as_ptr(), size_in_bytes) };
        self.state.take_error()?;
        check(status)
    }

    pub(crate) fn backend(&self) -> &B {
        &self.state.backend
    }

    pub(crate) fn backend_mut(&mut self) -> &mut B {
        &mut self.state.backend
    }
}

impl<B: Backend> Drop for NativeContext<B> {
    fn drop(&mut self) {
        // SAFETY: this context is uniquely owned, all callbacks are synchronous,
        // and the boxed callback state outlives the matching destructor call.
        unsafe { raw::bf_flow_context_destroy(self.pointer.as_ptr()) };
    }
}

pub(crate) fn flow_version() -> &'static str {
    // SAFETY: the wrapper returns either null or a process-lifetime version string.
    let pointer = unsafe { raw::bf_flow_version() };
    if pointer.is_null() {
        return "unknown";
    }
    // SAFETY: the non-null pointer above addresses the wrapper's NUL-terminated
    // process-lifetime version string.
    unsafe { CStr::from_ptr(pointer) }
        .to_str()
        .unwrap_or("unknown")
}

fn callbacks<B: Backend>(userdata: *mut c_void) -> raw::BFFlowBackendCallbacks {
    raw::BFFlowBackendCallbacks {
        userdata,
        get_current_frame: Some(get_current_frame::<B>),
        get_last_completed_frame: Some(get_last_completed_frame::<B>),
        is_feature_supported: Some(is_feature_supported::<B>),
        create_buffer: Some(create_buffer::<B>),
        destroy_buffer: Some(destroy_buffer::<B>),
        map_buffer: Some(map_buffer::<B>),
        unmap_buffer: Some(unmap_buffer::<B>),
        create_texture: Some(create_texture::<B>),
        destroy_texture: Some(destroy_texture::<B>),
        create_sampler: Some(create_sampler::<B>),
        destroy_sampler: Some(destroy_sampler::<B>),
        create_compute_pipeline: Some(create_compute_pipeline::<B>),
        destroy_compute_pipeline: Some(destroy_compute_pipeline::<B>),
        add_compute_pass: Some(add_compute_pass::<B>),
        add_copy_buffer_pass: Some(add_copy_buffer_pass::<B>),
        add_copy_buffer_to_texture_pass: Some(add_copy_buffer_to_texture_pass::<B>),
        add_copy_texture_to_buffer_pass: Some(add_copy_texture_to_buffer_pass::<B>),
        add_copy_texture_pass: Some(add_copy_texture_pass::<B>),
    }
}

unsafe fn state_mut<'a, B: Backend>(userdata: *mut c_void) -> &'a mut State<B> {
    &mut *userdata.cast::<State<B>>()
}

fn invoke<B: Backend, T>(
    userdata: *mut c_void,
    fallback: T,
    operation: impl FnOnce(&mut State<B>) -> Result<T, BackendError>,
) -> T {
    // SAFETY: every registered callback receives the stable userdata pointer
    // created from the live boxed `State<B>` and callbacks are serialized by Flow.
    let state = unsafe { state_mut::<B>(userdata) };
    match catch_unwind(AssertUnwindSafe(|| operation(state))) {
        Ok(Ok(value)) => value,
        Ok(Err(error)) => {
            state.record(error);
            fallback
        }
        Err(_payload) => {
            state.record(BackendError::Panicked);
            fallback
        }
    }
}

unsafe extern "C" fn get_current_frame<B: Backend>(userdata: *mut c_void) -> u64 {
    invoke::<B, _>(userdata, 0, |state| Ok(state.backend.current_frame()))
}

unsafe extern "C" fn get_last_completed_frame<B: Backend>(userdata: *mut c_void) -> u64 {
    invoke::<B, _>(userdata, 0, |state| {
        Ok(state.backend.last_completed_frame())
    })
}

unsafe extern "C" fn is_feature_supported<B: Backend>(userdata: *mut c_void, feature: u32) -> u8 {
    invoke::<B, _>(userdata, 0, |state| {
        Ok(u8::from(
            state.backend.feature_supported(feature_from_raw(feature)),
        ))
    })
}

unsafe extern "C" fn create_buffer<B: Backend>(
    userdata: *mut c_void,
    desc: *const raw::BFFlowBufferDesc,
) -> u64 {
    invoke::<B, _>(userdata, 0, |state| {
        let desc = desc
            .as_ref()
            .ok_or(BackendError::Rejected("create_buffer descriptor"))?;
        let size = usize::try_from(desc.size_in_bytes)
            .map_err(|_error| BackendError::Rejected("create_buffer size"))?;
        let id = state.backend.create_buffer(buffer_desc(desc))?;
        state.buffer_sizes.insert(id, size);
        Ok(id.get())
    })
}

unsafe extern "C" fn destroy_buffer<B: Backend>(userdata: *mut c_void, buffer: u64) {
    invoke::<B, _>(userdata, (), |state| {
        let id = required_id(buffer)?;
        state.backend.destroy_buffer(id)?;
        state.buffer_sizes.remove(&id);
        Ok(())
    });
}

unsafe extern "C" fn map_buffer<B: Backend>(userdata: *mut c_void, buffer: u64) -> *mut c_void {
    invoke::<B, _>(userdata, std::ptr::null_mut(), |state| {
        let id = required_id(buffer)?;
        let required = state.buffer_sizes.get(&id).copied().unwrap_or(0);
        let mapped = state.backend.map_buffer(id)?;
        if mapped.len() < required {
            return Err(BackendError::MappedBufferTooSmall);
        }
        Ok(mapped.as_mut_ptr().cast())
    })
}

unsafe extern "C" fn unmap_buffer<B: Backend>(userdata: *mut c_void, buffer: u64) {
    invoke::<B, _>(userdata, (), |state| {
        state.backend.unmap_buffer(required_id(buffer)?)
    });
}

unsafe extern "C" fn create_texture<B: Backend>(
    userdata: *mut c_void,
    desc: *const raw::BFFlowTextureDesc,
) -> u64 {
    invoke::<B, _>(userdata, 0, |state| {
        let desc = desc
            .as_ref()
            .ok_or(BackendError::Rejected("create_texture descriptor"))?;
        state
            .backend
            .create_texture(texture_desc(desc))
            .map(ResourceId::get)
    })
}

unsafe extern "C" fn destroy_texture<B: Backend>(userdata: *mut c_void, texture: u64) {
    invoke::<B, _>(userdata, (), |state| {
        state.backend.destroy_texture(required_id(texture)?)
    });
}

unsafe extern "C" fn create_sampler<B: Backend>(
    userdata: *mut c_void,
    desc: *const raw::BFFlowSamplerDesc,
) -> u64 {
    invoke::<B, _>(userdata, 0, |state| {
        let desc = desc
            .as_ref()
            .ok_or(BackendError::Rejected("create_sampler descriptor"))?;
        state
            .backend
            .create_sampler(sampler_desc(desc))
            .map(ResourceId::get)
    })
}

unsafe extern "C" fn destroy_sampler<B: Backend>(userdata: *mut c_void, sampler: u64) {
    invoke::<B, _>(userdata, (), |state| {
        state.backend.destroy_sampler(required_id(sampler)?)
    });
}

unsafe extern "C" fn create_compute_pipeline<B: Backend>(
    userdata: *mut c_void,
    desc: *const raw::BFFlowComputePipelineDesc,
) -> u64 {
    invoke::<B, _>(userdata, 0, |state| {
        let desc = desc
            .as_ref()
            .ok_or(BackendError::Rejected("create_compute_pipeline descriptor"))?;
        let raw_bindings = raw_slice(desc.bindings, desc.binding_count)?;
        let bindings = raw_bindings
            .iter()
            .map(|binding| BindingDesc {
                descriptor_type: DescriptorType::from_raw(binding.descriptor_type),
                binding: binding.binding,
                descriptor_count: binding.descriptor_count,
                set: binding.set,
            })
            .collect::<Vec<_>>();
        let bytecode_length = usize::try_from(desc.bytecode_size)
            .map_err(|_error| BackendError::Rejected("compute bytecode size"))?;
        let bytecode = if bytecode_length == 0 {
            &[]
        } else {
            if desc.bytecode.is_null() {
                return Err(BackendError::Rejected("compute bytecode pointer"));
            }
            slice::from_raw_parts(desc.bytecode, bytecode_length)
        };
        state
            .backend
            .create_compute_pipeline(ComputePipelineDesc {
                bindings: &bindings,
                bytecode,
            })
            .map(ResourceId::get)
    })
}

unsafe extern "C" fn destroy_compute_pipeline<B: Backend>(userdata: *mut c_void, pipeline: u64) {
    invoke::<B, _>(userdata, (), |state| {
        state
            .backend
            .destroy_compute_pipeline(required_id(pipeline)?)
    });
}

unsafe extern "C" fn add_compute_pass<B: Backend>(
    userdata: *mut c_void,
    pass: *const raw::BFFlowComputePass,
) -> u8 {
    invoke::<B, _>(userdata, 0, |state| {
        let pass = pass
            .as_ref()
            .ok_or(BackendError::Rejected("compute pass"))?;
        let raw_resources = raw_slice(pass.resources, pass.resource_count)?;
        let resources = raw_resources
            .iter()
            .map(|resource| ResourceBinding {
                descriptor_type: DescriptorType::from_raw(resource.descriptor_type),
                binding: resource.binding,
                array_index: resource.array_index,
                set: resource.set,
                buffer: optional_id(resource.buffer),
                texture: optional_id(resource.texture),
                sampler: optional_id(resource.sampler),
            })
            .collect::<Vec<_>>();
        state.backend.add_compute_pass(ComputePass {
            pipeline: required_id(pass.pipeline)?,
            grid: [pass.grid_x, pass.grid_y, pass.grid_z],
            resources: &resources,
            debug_label: label(pass.debug_label),
        })?;
        Ok(1)
    })
}

unsafe extern "C" fn add_copy_buffer_pass<B: Backend>(
    userdata: *mut c_void,
    pass: *const raw::BFFlowCopyBufferPass,
) -> u8 {
    invoke::<B, _>(userdata, 0, |state| {
        let pass = pass
            .as_ref()
            .ok_or(BackendError::Rejected("copy buffer pass"))?;
        state.backend.add_copy_buffer_pass(CopyBufferPass {
            source: required_id(pass.source)?,
            destination: required_id(pass.destination)?,
            source_offset: pass.source_offset,
            destination_offset: pass.destination_offset,
            size: pass.size,
            debug_label: label(pass.debug_label),
        })?;
        Ok(1)
    })
}

unsafe extern "C" fn add_copy_buffer_to_texture_pass<B: Backend>(
    userdata: *mut c_void,
    pass: *const raw::BFFlowBufferTextureCopyPass,
) -> u8 {
    invoke::<B, _>(userdata, 0, |state| {
        let pass = pass
            .as_ref()
            .ok_or(BackendError::Rejected("copy buffer to texture pass"))?;
        state
            .backend
            .add_copy_buffer_to_texture_pass(buffer_texture_copy(pass)?)?;
        Ok(1)
    })
}

unsafe extern "C" fn add_copy_texture_to_buffer_pass<B: Backend>(
    userdata: *mut c_void,
    pass: *const raw::BFFlowBufferTextureCopyPass,
) -> u8 {
    invoke::<B, _>(userdata, 0, |state| {
        let pass = pass
            .as_ref()
            .ok_or(BackendError::Rejected("copy texture to buffer pass"))?;
        state
            .backend
            .add_copy_texture_to_buffer_pass(buffer_texture_copy(pass)?)?;
        Ok(1)
    })
}

unsafe extern "C" fn add_copy_texture_pass<B: Backend>(
    userdata: *mut c_void,
    pass: *const raw::BFFlowCopyTexturePass,
) -> u8 {
    invoke::<B, _>(userdata, 0, |state| {
        let pass = pass
            .as_ref()
            .ok_or(BackendError::Rejected("copy texture pass"))?;
        state.backend.add_copy_texture_pass(CopyTexturePass {
            source: required_id(pass.source)?,
            destination: required_id(pass.destination)?,
            source_mip_level: pass.source_mip_level,
            source_offset: pass.source_offset,
            destination_mip_level: pass.destination_mip_level,
            destination_offset: pass.destination_offset,
            extent: pass.extent,
            debug_label: label(pass.debug_label),
        })?;
        Ok(1)
    })
}

unsafe fn raw_slice<'a, T>(pointer: *const T, count: u32) -> Result<&'a [T], BackendError> {
    let length =
        usize::try_from(count).map_err(|_error| BackendError::Rejected("Flow array length"))?;
    if length == 0 {
        Ok(&[])
    } else if pointer.is_null() {
        Err(BackendError::Rejected("Flow array pointer"))
    } else {
        Ok(slice::from_raw_parts(pointer, length))
    }
}

fn buffer_desc(desc: &raw::BFFlowBufferDesc) -> BufferDesc {
    BufferDesc {
        usage: BufferUsage::from_bits(desc.usage_flags),
        format: Format::from_raw(desc.format),
        structure_stride: desc.structure_stride,
        size_in_bytes: desc.size_in_bytes,
        memory_type: memory_type(desc.memory_type),
    }
}

fn texture_desc(desc: &raw::BFFlowTextureDesc) -> TextureDesc {
    TextureDesc {
        texture_type: texture_type(desc.texture_type),
        usage: TextureUsage::from_bits(desc.usage_flags),
        format: Format::from_raw(desc.format),
        width: desc.width,
        height: desc.height,
        depth: desc.depth,
        mip_levels: desc.mip_levels,
        optimized_clear_value: desc.optimized_clear_value,
    }
}

fn sampler_desc(desc: &raw::BFFlowSamplerDesc) -> SamplerDesc {
    SamplerDesc {
        address_mode_u: address_mode(desc.address_mode_u),
        address_mode_v: address_mode(desc.address_mode_v),
        address_mode_w: address_mode(desc.address_mode_w),
        filter_mode: filter_mode(desc.filter_mode),
    }
}

fn buffer_texture_copy(
    pass: &raw::BFFlowBufferTextureCopyPass,
) -> Result<BufferTextureCopyPass<'_>, BackendError> {
    Ok(BufferTextureCopyPass {
        buffer: required_id(pass.buffer)?,
        texture: required_id(pass.texture)?,
        buffer_offset: pass.buffer_offset,
        buffer_row_pitch: pass.buffer_row_pitch,
        buffer_depth_pitch: pass.buffer_depth_pitch,
        mip_level: pass.mip_level,
        offset: pass.offset,
        extent: pass.extent,
        debug_label: label(pass.debug_label),
    })
}

fn required_id(value: u64) -> Result<ResourceId, BackendError> {
    ResourceId::new(value).ok_or(BackendError::InvalidResourceId)
}

fn optional_id(value: u64) -> Option<ResourceId> {
    ResourceId::new(value)
}

fn feature_from_raw(value: u32) -> Feature {
    match value {
        raw::BF_FLOW_FEATURE_ALIAS_RESOURCE_FORMATS => Feature::AliasResourceFormats,
        raw::BF_FLOW_FEATURE_BUFFER_EXTERNAL_HANDLE => Feature::BufferExternalHandle,
        other => Feature::Unknown(other),
    }
}

fn memory_type(value: u32) -> MemoryType {
    match value {
        0 => MemoryType::Device,
        1 => MemoryType::Upload,
        2 => MemoryType::Readback,
        other => MemoryType::Unknown(other),
    }
}

fn texture_type(value: u32) -> TextureType {
    match value {
        0 => TextureType::OneDimensional,
        1 => TextureType::TwoDimensional,
        2 => TextureType::ThreeDimensional,
        other => TextureType::Unknown(other),
    }
}

fn address_mode(value: u32) -> AddressMode {
    match value {
        0 => AddressMode::Wrap,
        1 => AddressMode::Clamp,
        2 => AddressMode::Mirror,
        3 => AddressMode::BorderZero,
        other => AddressMode::Unknown(other),
    }
}

fn filter_mode(value: u32) -> FilterMode {
    match value {
        0 => FilterMode::Point,
        1 => FilterMode::Linear,
        other => FilterMode::Unknown(other),
    }
}

fn label<'a>(pointer: *const std::ffi::c_char) -> Option<&'a str> {
    if pointer.is_null() {
        None
    } else {
        // SAFETY: Flow callback descriptors supply either null or a valid
        // NUL-terminated label for the duration of the callback.
        unsafe { CStr::from_ptr(pointer) }.to_str().ok()
    }
}

fn check(status: i32) -> Result<(), Error> {
    if status == raw::BF_FLOW_STATUS_OK.cast_signed() {
        Ok(())
    } else if status == raw::BF_FLOW_STATUS_INVALID_ARGUMENT.cast_signed() {
        Err(Error::InvalidArgument)
    } else if status == raw::BF_FLOW_STATUS_ALLOCATION_FAILED.cast_signed() {
        Err(Error::AllocationFailed)
    } else if status == raw::BF_FLOW_STATUS_BACKEND_FAILED.cast_signed() {
        Err(Error::Backend(BackendError::Rejected(
            "native Flow operation",
        )))
    } else {
        Err(Error::NativeContract)
    }
}
