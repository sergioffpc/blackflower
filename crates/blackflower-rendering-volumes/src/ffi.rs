#![allow(
    unsafe_code,
    unsafe_op_in_unsafe_fn,
    reason = "all raw NanoVDB calls and pointer materialization are isolated in this private module"
)]
use std::ptr::NonNull;

use glam::{IVec3, Vec3};

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
#[allow(
    clippy::multiple_unsafe_ops_per_block,
    clippy::undocumented_unsafe_blocks,
    reason = "bindgen output is generated from the pinned NanoVDB wrapper headers"
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
pub(crate) struct HandlePtr(NonNull<raw::BFRenderingVolumesNanoVdb>);

// SAFETY: the native handle is immutable after construction, owns its buffers,
// and creates a fresh NanoVDB read accessor for every sampling call.
unsafe impl Send for HandlePtr {}
// SAFETY: all operations reachable through HandlePtr take a const native
// handle and do not share NanoVDB accessor caches between calls.
unsafe impl Sync for HandlePtr {}

pub(crate) fn openvdb_version() -> (u32, u32, u32) {
    // SAFETY: this wrapper query takes no pointers and returns a value record.
    let version = unsafe { raw::bf_rendering_volumes_openvdb_version() };
    (version.major, version.minor, version.patch)
}

pub(crate) fn vdb_version() -> (u32, u32, u32) {
    // SAFETY: this wrapper query takes no pointers and returns a value record.
    let version = unsafe { raw::bf_rendering_volumes_nanovdb_version() };
    (version.major, version.minor, version.patch)
}

pub(crate) fn load(bytes: &[u8]) -> Result<HandlePtr, Status> {
    let mut pointer = std::ptr::null_mut();
    // SAFETY: `bytes` is readable for its supplied length and `pointer` is a
    // valid, uniquely writable out-parameter.
    let status = unsafe {
        raw::bf_rendering_volumes_nanovdb_load(bytes.as_ptr(), bytes.len(), &raw mut pointer)
    };
    check(status)?;
    NonNull::new(pointer)
        .map(HandlePtr)
        .ok_or(Status::ContractViolation)
}

pub(crate) fn destroy(handle: HandlePtr) {
    // SAFETY: ownership of this live handle is transferred to `destroy`, which
    // calls the matching wrapper destructor exactly once.
    unsafe { raw::bf_rendering_volumes_nanovdb_destroy(handle.0.as_ptr()) };
}

pub(crate) fn grid_count(handle: HandlePtr) -> u32 {
    // SAFETY: `HandlePtr` can only contain a live, non-null native handle.
    unsafe { raw::bf_rendering_volumes_nanovdb_grid_count(handle.0.as_ptr()) }
}

pub(crate) fn grid_metadata(handle: HandlePtr, index: u32) -> Result<GridMetadata, Status> {
    let mut raw_info = raw::BFRenderingVolumesNanoVdbGridInfo::default();
    // SAFETY: the handle is live and `raw_info` is uniquely writable; the native
    // wrapper validates `index` before filling the record.
    let status = unsafe {
        raw::bf_rendering_volumes_nanovdb_grid_info(handle.0.as_ptr(), index, &raw mut raw_info)
    };
    check(status)?;

    let mut name_pointer = std::ptr::null();
    let mut name_length = 0;
    // SAFETY: the handle is live and both output slots are uniquely writable;
    // the wrapper validates `index` and borrows name storage from the handle.
    let status = unsafe {
        raw::bf_rendering_volumes_nanovdb_grid_name(
            handle.0.as_ptr(),
            index,
            &raw mut name_pointer,
            &raw mut name_length,
        )
    };
    check(status)?;
    // SAFETY: after a successful call the wrapper guarantees `name_pointer`
    // addresses `name_length` bytes owned by the still-live handle.
    let name_bytes = unsafe { std::slice::from_raw_parts(name_pointer.cast::<u8>(), name_length) };
    let name = std::str::from_utf8(name_bytes)
        .map_err(|_error| Status::InvalidAsset)?
        .to_owned();

    let grid_type = grid_type_from_raw(raw_info.grid_type).ok_or(Status::ContractViolation)?;
    let grid_class = grid_class_from_raw(raw_info.grid_class).ok_or(Status::ContractViolation)?;
    let empty = raw_info.is_empty != 0;
    let world_bounds = if empty {
        None
    } else {
        Some(Bounds3::new(
            safe_vec(raw_info.world_min)?,
            safe_vec(raw_info.world_max)?,
        ))
    };
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
        world_bounds,
        voxel_size: safe_vec(raw_info.voxel_size)?,
    })
}

pub(crate) fn index_to_world(
    handle: HandlePtr,
    index: u32,
    position: Vec3,
) -> Result<Vec3, Status> {
    transform(
        raw::bf_rendering_volumes_nanovdb_index_to_world,
        handle,
        index,
        position,
    )
}

pub(crate) fn world_to_index(
    handle: HandlePtr,
    index: u32,
    position: Vec3,
) -> Result<Vec3, Status> {
    transform(
        raw::bf_rendering_volumes_nanovdb_world_to_index,
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
    // SAFETY: the handle is live, the wrapper validates `index`, and both scalar
    // output slots are uniquely writable.
    let status = unsafe {
        raw::bf_rendering_volumes_nanovdb_float_voxel(
            handle.0.as_ptr(),
            index,
            raw::BFRenderingVolumesNanoVdbCoord {
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
    position: Vec3,
) -> Result<f32, Status> {
    let mut value = 0.0;
    // SAFETY: the handle is live, the wrapper validates `index`, and `value` is
    // a uniquely writable scalar output.
    let status = unsafe {
        raw::bf_rendering_volumes_nanovdb_sample_float_world(
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
        *const raw::BFRenderingVolumesNanoVdb,
        u32,
        raw::BFRenderingVolumesNanoVdbVec3d,
        *mut raw::BFRenderingVolumesNanoVdbVec3d,
    ) -> i32,
    handle: HandlePtr,
    index: u32,
    position: Vec3,
) -> Result<Vec3, Status> {
    let mut output = raw::BFRenderingVolumesNanoVdbVec3d::default();
    // SAFETY: callers pass one of the two pinned wrapper transform functions, a
    // live handle, and a uniquely writable output record; native code validates `index`.
    let status = unsafe { function(handle.0.as_ptr(), index, raw_vec(position), &raw mut output) };
    check(status)?;
    safe_vec(output)
}

fn check(status: i32) -> Result<(), Status> {
    let Ok(status) = u32::try_from(status) else {
        return Err(Status::ContractViolation);
    };
    match status {
        raw::BF_RENDERING_VOLUMES_NANOVDB_STATUS_OK => Ok(()),
        raw::BF_RENDERING_VOLUMES_NANOVDB_STATUS_INVALID_ARGUMENT => Err(Status::InvalidArgument),
        raw::BF_RENDERING_VOLUMES_NANOVDB_STATUS_INVALID_ASSET => Err(Status::InvalidAsset),
        raw::BF_RENDERING_VOLUMES_NANOVDB_STATUS_UNSUPPORTED_COMPRESSION => {
            Err(Status::UnsupportedCompression)
        }
        raw::BF_RENDERING_VOLUMES_NANOVDB_STATUS_OUT_OF_MEMORY => Err(Status::OutOfMemory),
        raw::BF_RENDERING_VOLUMES_NANOVDB_STATUS_INDEX_OUT_OF_RANGE => Err(Status::IndexOutOfRange),
        raw::BF_RENDERING_VOLUMES_NANOVDB_STATUS_TYPE_MISMATCH => Err(Status::TypeMismatch),
        raw::BF_RENDERING_VOLUMES_NANOVDB_STATUS_NATIVE_FAILURE => Err(Status::NativeFailure),
        _ => Err(Status::ContractViolation),
    }
}

const fn grid_type_from_raw(value: u32) -> Option<GridType> {
    Some(match value {
        raw::BF_RENDERING_VOLUMES_NANOVDB_GRID_TYPE_UNKNOWN => GridType::Unknown,
        raw::BF_RENDERING_VOLUMES_NANOVDB_GRID_TYPE_FLOAT => GridType::Float,
        raw::BF_RENDERING_VOLUMES_NANOVDB_GRID_TYPE_DOUBLE => GridType::Double,
        raw::BF_RENDERING_VOLUMES_NANOVDB_GRID_TYPE_INT16 => GridType::Int16,
        raw::BF_RENDERING_VOLUMES_NANOVDB_GRID_TYPE_INT32 => GridType::Int32,
        raw::BF_RENDERING_VOLUMES_NANOVDB_GRID_TYPE_INT64 => GridType::Int64,
        raw::BF_RENDERING_VOLUMES_NANOVDB_GRID_TYPE_VEC3F => GridType::Vec3f,
        raw::BF_RENDERING_VOLUMES_NANOVDB_GRID_TYPE_VEC3D => GridType::Vec3d,
        raw::BF_RENDERING_VOLUMES_NANOVDB_GRID_TYPE_MASK => GridType::Mask,
        raw::BF_RENDERING_VOLUMES_NANOVDB_GRID_TYPE_HALF => GridType::Half,
        raw::BF_RENDERING_VOLUMES_NANOVDB_GRID_TYPE_UINT32 => GridType::UInt32,
        raw::BF_RENDERING_VOLUMES_NANOVDB_GRID_TYPE_BOOLEAN => GridType::Boolean,
        raw::BF_RENDERING_VOLUMES_NANOVDB_GRID_TYPE_RGBA8 => GridType::Rgba8,
        raw::BF_RENDERING_VOLUMES_NANOVDB_GRID_TYPE_FP4 => GridType::Fp4,
        raw::BF_RENDERING_VOLUMES_NANOVDB_GRID_TYPE_FP8 => GridType::Fp8,
        raw::BF_RENDERING_VOLUMES_NANOVDB_GRID_TYPE_FP16 => GridType::Fp16,
        raw::BF_RENDERING_VOLUMES_NANOVDB_GRID_TYPE_FPN => GridType::FpN,
        raw::BF_RENDERING_VOLUMES_NANOVDB_GRID_TYPE_VEC4F => GridType::Vec4f,
        raw::BF_RENDERING_VOLUMES_NANOVDB_GRID_TYPE_VEC4D => GridType::Vec4d,
        raw::BF_RENDERING_VOLUMES_NANOVDB_GRID_TYPE_INDEX => GridType::Index,
        raw::BF_RENDERING_VOLUMES_NANOVDB_GRID_TYPE_ON_INDEX => GridType::OnIndex,
        raw::BF_RENDERING_VOLUMES_NANOVDB_GRID_TYPE_POINT_INDEX => GridType::PointIndex,
        raw::BF_RENDERING_VOLUMES_NANOVDB_GRID_TYPE_VEC3U8 => GridType::Vec3u8,
        raw::BF_RENDERING_VOLUMES_NANOVDB_GRID_TYPE_VEC3U16 => GridType::Vec3u16,
        raw::BF_RENDERING_VOLUMES_NANOVDB_GRID_TYPE_UINT8 => GridType::UInt8,
        _ => return None,
    })
}

const fn grid_class_from_raw(value: u32) -> Option<GridClass> {
    Some(match value {
        raw::BF_RENDERING_VOLUMES_NANOVDB_GRID_CLASS_UNKNOWN => GridClass::Unknown,
        raw::BF_RENDERING_VOLUMES_NANOVDB_GRID_CLASS_LEVEL_SET => GridClass::LevelSet,
        raw::BF_RENDERING_VOLUMES_NANOVDB_GRID_CLASS_FOG_VOLUME => GridClass::FogVolume,
        raw::BF_RENDERING_VOLUMES_NANOVDB_GRID_CLASS_STAGGERED => GridClass::Staggered,
        raw::BF_RENDERING_VOLUMES_NANOVDB_GRID_CLASS_POINT_INDEX => GridClass::PointIndex,
        raw::BF_RENDERING_VOLUMES_NANOVDB_GRID_CLASS_POINT_DATA => GridClass::PointData,
        raw::BF_RENDERING_VOLUMES_NANOVDB_GRID_CLASS_TOPOLOGY => GridClass::Topology,
        raw::BF_RENDERING_VOLUMES_NANOVDB_GRID_CLASS_VOXEL_VOLUME => GridClass::VoxelVolume,
        raw::BF_RENDERING_VOLUMES_NANOVDB_GRID_CLASS_INDEX_GRID => GridClass::IndexGrid,
        raw::BF_RENDERING_VOLUMES_NANOVDB_GRID_CLASS_TENSOR_GRID => GridClass::TensorGrid,
        _ => return None,
    })
}

fn raw_vec(value: Vec3) -> raw::BFRenderingVolumesNanoVdbVec3d {
    raw::BFRenderingVolumesNanoVdbVec3d {
        x: f64::from(value.x),
        y: f64::from(value.y),
        z: f64::from(value.z),
    }
}

#[allow(
    clippy::cast_possible_truncation,
    reason = "NanoVDB double coordinates enter the canonical f32 rendering domain at this private FFI boundary"
)]
fn safe_vec(value: raw::BFRenderingVolumesNanoVdbVec3d) -> Result<Vec3, Status> {
    let converted = Vec3::new(value.x as f32, value.y as f32, value.z as f32);
    if converted.is_finite() {
        Ok(converted)
    } else {
        Err(Status::ContractViolation)
    }
}

const fn safe_coord(value: raw::BFRenderingVolumesNanoVdbCoord) -> IVec3 {
    IVec3::new(value.x, value.y, value.z)
}
