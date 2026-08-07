#![allow(
    unsafe_code,
    unsafe_op_in_unsafe_fn,
    reason = "all raw Embree wrapper calls and pointer materialization are isolated in this private module"
)]
use std::mem::MaybeUninit;
use std::ptr::NonNull;

use glam::Vec3A;

use crate::{Error, SurfaceHit, Triangle};

#[allow(
    dead_code,
    non_camel_case_types,
    non_snake_case,
    non_upper_case_globals,
    unsafe_code,
    reason = "generated declarations mirror the Blackflower Embree C wrapper"
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
    reason = "bindgen output is generated from the pinned Embree wrapper headers"
)]
mod raw {
    include!(concat!(env!("OUT_DIR"), "/spatial_query_bindings.rs"));
}

#[derive(Debug, Clone, Copy)]
pub(crate) struct DevicePtr(NonNull<raw::BFSpatialQueryDevice>);

#[derive(Debug, Clone, Copy)]
pub(crate) struct ScenePtr(NonNull<raw::BFSpatialQueryScene>);

// SAFETY: Embree devices are internally thread-safe and the wrapper exposes no
// mutable Rust references through this opaque pointer.
unsafe impl Send for DevicePtr {}
// SAFETY: Embree devices support concurrent calls and their lifetime is owned
// externally by the safe `Device` wrapper.
unsafe impl Sync for DevicePtr {}
// SAFETY: committed Embree scenes and wrapper mutation are synchronized by the
// safe owner; moving the opaque pointer does not transfer borrowed data.
unsafe impl Send for ScenePtr {}
// SAFETY: queries use Embree's concurrent scene API and the safe owner excludes
// mutation while shared queries are possible.
unsafe impl Sync for ScenePtr {}

const _: () = assert!(
    std::mem::size_of::<SurfaceHit>() == std::mem::size_of::<raw::BFSpatialQuerySurfaceHit>()
);
const _: () = assert!(
    std::mem::align_of::<SurfaceHit>() == std::mem::align_of::<raw::BFSpatialQuerySurfaceHit>()
);

pub(crate) fn embree_version() -> (u32, u32, u32) {
    // SAFETY: this wrapper query takes no pointers and returns a value record.
    let version = unsafe { raw::bf_spatial_query_embree_version() };
    (version.major, version.minor, version.patch)
}

pub(crate) fn create_device() -> Result<DevicePtr, Error> {
    let mut pointer = std::ptr::null_mut();
    // SAFETY: `pointer` is a valid, uniquely writable out-parameter.
    let status = unsafe { raw::bf_spatial_query_device_create(&raw mut pointer) };
    check(status)?;
    NonNull::new(pointer)
        .map(DevicePtr)
        .ok_or(Error::ContractViolation)
}

pub(crate) fn destroy_device(device: DevicePtr) {
    // SAFETY: ownership of this live device is transferred here and the safe
    // owner destroys all dependent scenes before invoking this function.
    unsafe { raw::bf_spatial_query_device_destroy(device.0.as_ptr()) };
}

pub(crate) fn create_scene(device: DevicePtr) -> Result<ScenePtr, Error> {
    let mut pointer = std::ptr::null_mut();
    // SAFETY: `device` is live and `pointer` is a valid, uniquely writable out-parameter.
    let status = unsafe { raw::bf_spatial_query_scene_create(device.0.as_ptr(), &raw mut pointer) };
    check(status)?;
    NonNull::new(pointer)
        .map(ScenePtr)
        .ok_or(Error::ContractViolation)
}

pub(crate) fn destroy_scene(scene: ScenePtr) {
    // SAFETY: ownership of this live scene is transferred here and it is
    // destroyed exactly once by its safe owner.
    unsafe { raw::bf_spatial_query_scene_destroy(scene.0.as_ptr()) };
}

pub(crate) fn add_triangles(scene: ScenePtr, triangles: &[Triangle]) -> Result<u32, Error> {
    let count =
        u32::try_from(triangles.len()).map_err(|_error| Error::ResourceLimit("triangles"))?;
    let raw_triangles = triangles
        .iter()
        .copied()
        .map(|triangle| raw::BFSpatialQueryTriangle {
            vertices: triangle.vertices().map(raw_vec3),
        })
        .collect::<Vec<_>>();
    let mut geometry_id = u32::MAX;
    // SAFETY: the scene is live, `raw_triangles` remains readable for `count`
    // entries during the call, and `geometry_id` is uniquely writable.
    let status = unsafe {
        raw::bf_spatial_query_scene_add_triangles(
            scene.0.as_ptr(),
            raw_triangles.as_ptr(),
            count,
            &raw mut geometry_id,
        )
    };
    check(status)?;
    if geometry_id == u32::MAX {
        Err(Error::ContractViolation)
    } else {
        Ok(geometry_id)
    }
}

pub(crate) fn commit_scene(scene: ScenePtr) -> Result<(), Error> {
    // SAFETY: `scene` is live and the safe owner serializes mutation/commit.
    let status = unsafe { raw::bf_spatial_query_scene_commit(scene.0.as_ptr()) };
    check(status)
}

pub(crate) fn intersect_segment(
    scene: ScenePtr,
    start: Vec3A,
    end: Vec3A,
    max_hits: usize,
    output: &mut Vec<SurfaceHit>,
) -> Result<(), Error> {
    output.clear();
    let capacity =
        u32::try_from(max_hits).map_err(|_error| Error::ResourceLimit("segment hits"))?;
    if capacity == 0 {
        return Ok(());
    }
    output
        .try_reserve(max_hits)
        .map_err(|_error| Error::OutOfMemory)?;
    let mut count = 0_u32;
    // SAFETY: the committed scene is live, the vector has capacity for
    // `capacity` hits, and `count` is a uniquely writable out-parameter.
    let status = unsafe {
        raw::bf_spatial_query_scene_intersect_segment(
            scene.0.as_ptr(),
            raw_vec3(start),
            raw_vec3(end),
            capacity,
            output.as_mut_ptr().cast(),
            &raw mut count,
        )
    };
    check(status)?;
    if count > capacity {
        return Err(Error::ContractViolation);
    }
    // SAFETY: the successful wrapper call initialized exactly `count` hit
    // records, and the count was checked against the reserved capacity.
    unsafe { output.set_len(usize::try_from(count).unwrap_or(0)) };
    Ok(())
}

pub(crate) fn closest_hit(
    scene: ScenePtr,
    start: Vec3A,
    end: Vec3A,
) -> Result<Option<SurfaceHit>, Error> {
    let mut hit = MaybeUninit::<SurfaceHit>::uninit();
    let mut has_hit = 0_u8;
    // SAFETY: the committed scene is live and both output slots are uniquely
    // writable; the wrapper initializes `hit` iff it reports `has_hit == 1`.
    let status = unsafe {
        raw::bf_spatial_query_scene_closest_hit(
            scene.0.as_ptr(),
            raw_vec3(start),
            raw_vec3(end),
            hit.as_mut_ptr().cast(),
            &raw mut has_hit,
        )
    };
    check(status)?;
    match has_hit {
        0 => Ok(None),
        1 => {
            // SAFETY: the wrapper contract initializes `hit` whenever it returns
            // success with `has_hit == 1`, established by this match arm.
            Ok(Some(unsafe { hit.assume_init() }))
        }
        _ => Err(Error::ContractViolation),
    }
}

pub(crate) fn is_occluded(scene: ScenePtr, start: Vec3A, end: Vec3A) -> Result<bool, Error> {
    let mut occluded = 0_u8;
    // SAFETY: the committed scene is live and `occluded` is a uniquely writable
    // scalar output validated below before conversion to `bool`.
    let status = unsafe {
        raw::bf_spatial_query_scene_is_occluded(
            scene.0.as_ptr(),
            raw_vec3(start),
            raw_vec3(end),
            &raw mut occluded,
        )
    };
    check(status)?;
    match occluded {
        0 => Ok(false),
        1 => Ok(true),
        _ => Err(Error::ContractViolation),
    }
}

fn raw_vec3(value: Vec3A) -> raw::BFSpatialQueryVec3 {
    raw::BFSpatialQueryVec3 {
        x: value.x,
        y: value.y,
        z: value.z,
    }
}

fn check(status: i32) -> Result<(), Error> {
    match status {
        value if value == raw::BF_SPATIAL_QUERY_STATUS_OK.cast_signed() => Ok(()),
        value if value == raw::BF_SPATIAL_QUERY_STATUS_INVALID_ARGUMENT.cast_signed() => {
            Err(Error::ContractViolation)
        }
        value if value == raw::BF_SPATIAL_QUERY_STATUS_OUT_OF_MEMORY.cast_signed() => {
            Err(Error::NativeOutOfMemory)
        }
        value if value == raw::BF_SPATIAL_QUERY_STATUS_NATIVE_FAILURE.cast_signed() => {
            Err(Error::NativeFailure)
        }
        value if value == raw::BF_SPATIAL_QUERY_STATUS_SCENE_COMMITTED.cast_signed() => {
            Err(Error::SceneCommitted)
        }
        _ => Err(Error::ContractViolation),
    }
}
