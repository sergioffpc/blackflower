#![allow(
    unsafe_code,
    unsafe_op_in_unsafe_fn,
    reason = "all raw NanoVDB calls and pointer materialization are isolated in this private module"
)]
#![allow(
    clippy::undocumented_unsafe_blocks,
    clippy::multiple_unsafe_ops_per_block,
    reason = "all unsafe operations are confined to the reviewed NanoVDB FFI boundary"
)]

use std::ptr::NonNull;

use glam::{DVec3, IVec3};

use crate::types::{Bounds3, FloatVoxel, GridClass, GridMetadata, GridType};

#[allow(
    dead_code,
    non_camel_case_types,
    non_snake_case,
    non_upper_case_globals,
    unsafe_code,
    reason = "generated declarations mirror the blackflower NanoVDB C wrapper"
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
pub(crate) mod raw {
    include!(concat!(env!("OUT_DIR"), "/nanovdb_bindings.rs"));
}

#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub(crate) enum Status {
    InvalidArgument,
    InvalidAsset,
    UnsupportedCompression,
    OutOfMemory,
    IndexOutOfRange,
    TypeMismatch,
    NativeFailure,
    ContractViolation,
}

#[derive(Debug, Clone, Copy)]
pub(crate) struct HandlePtr(NonNull<raw::BFRenderNanoVdb>);

// SAFETY: the native handle is immutable after construction, owns its buffers,
// and creates a fresh NanoVDB read accessor for every sampling call.
unsafe impl Send for HandlePtr {}
// SAFETY: all operations reachable through HandlePtr take a const native
// handle and do not share NanoVDB accessor caches between calls.
unsafe impl Sync for HandlePtr {}

pub(crate) fn openvdb_version() -> (u32, u32, u32) {
    let version = unsafe { raw::bf_render_openvdb_version() };
    (version.major, version.minor, version.patch)
}

pub(crate) fn nanovdb_version() -> (u32, u32, u32) {
    let version = unsafe { raw::bf_render_nanovdb_version() };
    (version.major, version.minor, version.patch)
}

pub(crate) fn load(bytes: &[u8]) -> Result<HandlePtr, Status> {
    let mut pointer = std::ptr::null_mut();
    let status =
        unsafe { raw::bf_render_nanovdb_load(bytes.as_ptr(), bytes.len(), &raw mut pointer) };
    check(status)?;
    NonNull::new(pointer)
        .map(HandlePtr)
        .ok_or(Status::ContractViolation)
}

pub(crate) fn destroy(handle: HandlePtr) {
    unsafe { raw::bf_render_nanovdb_destroy(handle.0.as_ptr()) };
}

pub(crate) fn grid_count(handle: HandlePtr) -> u32 {
    unsafe { raw::bf_render_nanovdb_grid_count(handle.0.as_ptr()) }
}

pub(crate) fn grid_metadata(handle: HandlePtr, index: u32) -> Result<GridMetadata, Status> {
    let mut raw_info = raw::BFRenderNanoVdbGridInfo::default();
    let status =
        unsafe { raw::bf_render_nanovdb_grid_info(handle.0.as_ptr(), index, &raw mut raw_info) };
    check(status)?;

    let mut name_pointer = std::ptr::null();
    let mut name_length = 0;
    let status = unsafe {
        raw::bf_render_nanovdb_grid_name(
            handle.0.as_ptr(),
            index,
            &raw mut name_pointer,
            &raw mut name_length,
        )
    };
    check(status)?;
    let name_bytes = unsafe { std::slice::from_raw_parts(name_pointer.cast::<u8>(), name_length) };
    let name = std::str::from_utf8(name_bytes)
        .map_err(|_error| Status::InvalidAsset)?
        .to_owned();

    let grid_type = GridType::from_raw(raw_info.grid_type).ok_or(Status::ContractViolation)?;
    let grid_class = GridClass::from_raw(raw_info.grid_class).ok_or(Status::ContractViolation)?;
    let empty = raw_info.is_empty != 0;
    Ok(GridMetadata {
        name,
        grid_type,
        grid_class,
        byte_size: raw_info.byte_size,
        active_voxel_count: raw_info.active_voxel_count,
        index_bounds: (!empty).then(|| {
            Bounds3::new(
                safe_coord(raw_info.index_min),
                safe_coord(raw_info.index_max),
            )
        }),
        world_bounds: (!empty)
            .then(|| Bounds3::new(safe_vec(raw_info.world_min), safe_vec(raw_info.world_max))),
        voxel_size: safe_vec(raw_info.voxel_size),
    })
}

pub(crate) fn index_to_world(
    handle: HandlePtr,
    index: u32,
    position: DVec3,
) -> Result<DVec3, Status> {
    transform(
        raw::bf_render_nanovdb_index_to_world,
        handle,
        index,
        position,
    )
}

pub(crate) fn world_to_index(
    handle: HandlePtr,
    index: u32,
    position: DVec3,
) -> Result<DVec3, Status> {
    transform(
        raw::bf_render_nanovdb_world_to_index,
        handle,
        index,
        position,
    )
}

pub(crate) fn float_voxel(
    handle: HandlePtr,
    index: u32,
    coordinate: IVec3,
) -> Result<FloatVoxel, Status> {
    let mut value = 0.0;
    let mut active = 0;
    let status = unsafe {
        raw::bf_render_nanovdb_float_voxel(
            handle.0.as_ptr(),
            index,
            raw::BFRenderNanoVdbCoord {
                x: coordinate.x,
                y: coordinate.y,
                z: coordinate.z,
            },
            &raw mut value,
            &raw mut active,
        )
    };
    check(status)?;
    Ok(FloatVoxel::new(value, active != 0))
}

pub(crate) fn sample_float_world(
    handle: HandlePtr,
    index: u32,
    position: DVec3,
) -> Result<f32, Status> {
    let mut value = 0.0;
    let status = unsafe {
        raw::bf_render_nanovdb_sample_float_world(
            handle.0.as_ptr(),
            index,
            raw_vec(position),
            &raw mut value,
        )
    };
    check(status)?;
    Ok(value)
}

fn transform(
    function: unsafe extern "C" fn(
        *const raw::BFRenderNanoVdb,
        u32,
        raw::BFRenderNanoVdbVec3d,
        *mut raw::BFRenderNanoVdbVec3d,
    ) -> i32,
    handle: HandlePtr,
    index: u32,
    position: DVec3,
) -> Result<DVec3, Status> {
    let mut output = raw::BFRenderNanoVdbVec3d::default();
    let status = unsafe { function(handle.0.as_ptr(), index, raw_vec(position), &raw mut output) };
    check(status)?;
    Ok(safe_vec(output))
}

fn check(status: i32) -> Result<(), Status> {
    let Ok(status) = u32::try_from(status) else {
        return Err(Status::ContractViolation);
    };
    match status {
        raw::BF_RENDER_NANOVDB_STATUS_OK => Ok(()),
        raw::BF_RENDER_NANOVDB_STATUS_INVALID_ARGUMENT => Err(Status::InvalidArgument),
        raw::BF_RENDER_NANOVDB_STATUS_INVALID_ASSET => Err(Status::InvalidAsset),
        raw::BF_RENDER_NANOVDB_STATUS_UNSUPPORTED_COMPRESSION => {
            Err(Status::UnsupportedCompression)
        }
        raw::BF_RENDER_NANOVDB_STATUS_OUT_OF_MEMORY => Err(Status::OutOfMemory),
        raw::BF_RENDER_NANOVDB_STATUS_INDEX_OUT_OF_RANGE => Err(Status::IndexOutOfRange),
        raw::BF_RENDER_NANOVDB_STATUS_TYPE_MISMATCH => Err(Status::TypeMismatch),
        raw::BF_RENDER_NANOVDB_STATUS_NATIVE_FAILURE => Err(Status::NativeFailure),
        _ => Err(Status::ContractViolation),
    }
}

const fn raw_vec(value: DVec3) -> raw::BFRenderNanoVdbVec3d {
    raw::BFRenderNanoVdbVec3d {
        x: value.x,
        y: value.y,
        z: value.z,
    }
}

const fn safe_vec(value: raw::BFRenderNanoVdbVec3d) -> DVec3 {
    DVec3::new(value.x, value.y, value.z)
}

const fn safe_coord(value: raw::BFRenderNanoVdbCoord) -> IVec3 {
    IVec3::new(value.x, value.y, value.z)
}
