#![allow(
    unsafe_code,
    unsafe_op_in_unsafe_fn,
    reason = "all raw Detour calls and pointer materialization are isolated in this private module"
)]
use std::ptr::NonNull;

use glam::Vec3A;

use crate::filter::QueryFilter;
use crate::navmesh::NavMeshParams;

#[allow(
    dead_code,
    non_camel_case_types,
    non_snake_case,
    non_upper_case_globals,
    unsafe_code,
    reason = "generated declarations mirror the blackflower Detour C wrapper"
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
    reason = "bindgen output is generated from the pinned Detour wrapper headers"
)]
pub(crate) mod raw {
    include!(concat!(env!("OUT_DIR"), "/recastnavigation_bindings.rs"));
}

#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub(crate) enum Status {
    InvalidArgument,
    OutOfMemory,
    InvalidNavMeshData,
    InitializationFailed,
    TileAlreadyOccupied,
    QueryFailed,
    ContractViolation,
}

#[derive(Debug, Clone, Copy)]
pub(crate) struct NavMeshPtr(NonNull<raw::BFNavigationNavMesh>);

#[derive(Debug, Clone, Copy)]
pub(crate) struct QueryPtr(NonNull<raw::BFNavigationQuery>);

#[derive(Debug, Clone, Copy, Default)]
pub(crate) struct Details(u32);

impl Details {
    pub(crate) const fn buffer_too_small(self) -> bool {
        self.0 & raw::BF_NAVIGATION_DETAIL_BUFFER_TOO_SMALL != 0
    }

    pub(crate) const fn out_of_nodes(self) -> bool {
        self.0 & raw::BF_NAVIGATION_DETAIL_OUT_OF_NODES != 0
    }

    pub(crate) const fn partial_result(self) -> bool {
        self.0 & raw::BF_NAVIGATION_DETAIL_PARTIAL_RESULT != 0
    }
}

#[derive(Debug, Clone, Copy)]
pub(crate) struct Nearest {
    pub(crate) polygon: u32,
    pub(crate) position: Vec3A,
    pub(crate) is_over_polygon: bool,
}

#[derive(Debug, Clone, Copy)]
pub(crate) struct Raycast {
    pub(crate) fraction: f32,
    pub(crate) normal: Vec3A,
    pub(crate) edge_index: i32,
    pub(crate) path_cost: f32,
}

pub(crate) fn recastnavigation_version() -> (u32, u32, u32) {
    // SAFETY: this wrapper query takes no pointers and returns a value record.
    let version = unsafe { raw::bf_navigation_recast_version() };
    (version.major, version.minor, version.patch)
}

pub(crate) fn detour_navmesh_version() -> u32 {
    // SAFETY: this wrapper query takes no pointers and returns a value.
    unsafe { raw::bf_navigation_detour_navmesh_version() }
}

pub(crate) fn create_single_tile(data: &[u8]) -> Result<NavMeshPtr, Status> {
    let mut pointer = std::ptr::null_mut();
    // SAFETY: `data` is readable for its supplied length and `pointer` is a
    // valid, uniquely writable out-parameter.
    let status = unsafe {
        raw::bf_navigation_navmesh_create_single_tile(data.as_ptr(), data.len(), &raw mut pointer)
    };
    check(status)?;
    NonNull::new(pointer)
        .map(NavMeshPtr)
        .ok_or(Status::ContractViolation)
}

pub(crate) fn create_tiled(params: NavMeshParams) -> Result<NavMeshPtr, Status> {
    let params = raw::BFNavigationNavMeshParams {
        origin: raw_vec(params.origin),
        tile_width: params.tile_width,
        tile_height: params.tile_height,
        max_tiles: params.max_tiles.get(),
        max_polygons_per_tile: params.max_polygons_per_tile.get(),
    };
    let mut pointer = std::ptr::null_mut();
    // SAFETY: `params` is a fully initialized input record and `pointer` is a
    // valid, uniquely writable out-parameter.
    let status =
        unsafe { raw::bf_navigation_navmesh_create_tiled(&raw const params, &raw mut pointer) };
    check(status)?;
    NonNull::new(pointer)
        .map(NavMeshPtr)
        .ok_or(Status::ContractViolation)
}

pub(crate) fn destroy_navmesh(navmesh: NavMeshPtr) {
    // SAFETY: ownership of this live navmesh is transferred here and the safe
    // owner has destroyed all queries before invoking the matching destructor.
    unsafe { raw::bf_navigation_navmesh_destroy(navmesh.0.as_ptr()) };
}

pub(crate) fn add_tile(navmesh: NavMeshPtr, data: &[u8]) -> Result<u32, Status> {
    let mut reference = 0;
    // SAFETY: the navmesh is live, `data` remains readable for its supplied
    // length, and `reference` is uniquely writable.
    let status = unsafe {
        raw::bf_navigation_navmesh_add_tile(
            navmesh.0.as_ptr(),
            data.as_ptr(),
            data.len(),
            0,
            &raw mut reference,
        )
    };
    check(status)?;
    if reference == 0 {
        Err(Status::ContractViolation)
    } else {
        Ok(reference)
    }
}

pub(crate) fn remove_tile(navmesh: NavMeshPtr, reference: u32) -> Result<(), Status> {
    // SAFETY: the navmesh is live and the wrapper validates the opaque tile reference.
    let status = unsafe { raw::bf_navigation_navmesh_remove_tile(navmesh.0.as_ptr(), reference) };
    check(status)
}

pub(crate) fn replace_tile(
    navmesh: NavMeshPtr,
    reference: u32,
    data: &[u8],
) -> Result<u32, Status> {
    let mut replaced_reference = 0;
    // SAFETY: the navmesh is live, the wrapper validates `reference`, `data` is
    // readable for its supplied length, and the output slot is uniquely writable.
    let status = unsafe {
        raw::bf_navigation_navmesh_replace_tile(
            navmesh.0.as_ptr(),
            reference,
            data.as_ptr(),
            data.len(),
            &raw mut replaced_reference,
        )
    };
    check(status)?;
    if replaced_reference == 0 {
        Err(Status::ContractViolation)
    } else {
        Ok(replaced_reference)
    }
}

pub(crate) fn create_query(navmesh: NavMeshPtr, max_nodes: u32) -> Result<QueryPtr, Status> {
    let mut pointer = std::ptr::null_mut();
    // SAFETY: the navmesh remains live for the query lifetime and `pointer` is
    // a valid, uniquely writable out-parameter.
    let status =
        unsafe { raw::bf_navigation_query_create(navmesh.0.as_ptr(), max_nodes, &raw mut pointer) };
    check(status)?;
    NonNull::new(pointer)
        .map(QueryPtr)
        .ok_or(Status::ContractViolation)
}

pub(crate) fn destroy_query(query: QueryPtr) {
    // SAFETY: ownership of this live query is transferred here and it is
    // destroyed exactly once by its safe owner.
    unsafe { raw::bf_navigation_query_destroy(query.0.as_ptr()) };
}

pub(crate) fn find_nearest_point(
    query: QueryPtr,
    center: Vec3A,
    half_extents: Vec3A,
    filter: &QueryFilter,
) -> Result<Option<Nearest>, Status> {
    let mut nearest = raw::BFNavigationNearestPoint::default();
    let filter = raw_filter(filter);
    // SAFETY: the query is live and tied to its navmesh; input records remain
    // readable and `nearest` is uniquely writable for the call.
    let status = unsafe {
        raw::bf_navigation_query_find_nearest_point(
            query.0.as_ptr(),
            raw_vec(center),
            raw_vec(half_extents),
            &raw const filter,
            &raw mut nearest,
        )
    };
    check(status)?;
    if nearest.polygon == 0 {
        Ok(None)
    } else {
        Ok(Some(Nearest {
            polygon: nearest.polygon,
            position: safe_vec(nearest.position),
            is_over_polygon: nearest.is_over_polygon != 0,
        }))
    }
}

pub(crate) fn closest_point_on_polygon(
    query: QueryPtr,
    polygon: u32,
    position: Vec3A,
) -> Result<Nearest, Status> {
    let mut closest = raw::BFNavigationNearestPoint::default();
    // SAFETY: the query is live, the wrapper validates `polygon`, and `closest`
    // is a uniquely writable output record.
    let status = unsafe {
        raw::bf_navigation_query_closest_point_on_polygon(
            query.0.as_ptr(),
            polygon,
            raw_vec(position),
            &raw mut closest,
        )
    };
    check(status)?;
    if closest.polygon == 0 {
        Err(Status::ContractViolation)
    } else {
        Ok(Nearest {
            polygon: closest.polygon,
            position: safe_vec(closest.position),
            is_over_polygon: closest.is_over_polygon != 0,
        })
    }
}

pub(crate) fn find_path(
    query: QueryPtr,
    start_polygon: u32,
    end_polygon: u32,
    start: Vec3A,
    end: Vec3A,
    filter: &QueryFilter,
    path: &mut [u32],
) -> Result<(usize, Details), Status> {
    let capacity = u32::try_from(path.len()).map_err(|_error| Status::InvalidArgument)?;
    let filter = raw_filter(filter);
    let mut count = 0;
    let mut details = 0;
    // SAFETY: the query is live, the wrapper validates polygon references, the
    // path buffer is writable for `capacity`, and scalar outputs are distinct.
    let status = unsafe {
        raw::bf_navigation_query_find_path(
            query.0.as_ptr(),
            start_polygon,
            end_polygon,
            raw_vec(start),
            raw_vec(end),
            &raw const filter,
            path.as_mut_ptr(),
            capacity,
            &raw mut count,
            &raw mut details,
        )
    };
    check(status)?;
    let count = checked_count(count, path.len())?;
    Ok((count, Details(details)))
}

pub(crate) struct StraightPathBuffers<'a> {
    pub(crate) points: &'a mut [raw::BFNavigationVec3],
    pub(crate) flags: &'a mut [u8],
    pub(crate) polygons: &'a mut [u32],
}

pub(crate) fn find_straight_path(
    query: QueryPtr,
    start: Vec3A,
    end: Vec3A,
    corridor: &[u32],
    buffers: StraightPathBuffers<'_>,
) -> Result<(usize, Details), Status> {
    if buffers.points.len() != buffers.flags.len() || buffers.points.len() != buffers.polygons.len()
    {
        return Err(Status::ContractViolation);
    }
    let path_count = u32::try_from(corridor.len()).map_err(|_error| Status::InvalidArgument)?;
    let capacity = u32::try_from(buffers.points.len()).map_err(|_error| Status::InvalidArgument)?;
    let mut count = 0;
    let mut details = 0;
    // SAFETY: the query is live, `corridor` is readable for `path_count`, the
    // three disjoint output buffers share the checked capacity, and scalar outputs are distinct.
    let status = unsafe {
        raw::bf_navigation_query_find_straight_path(
            query.0.as_ptr(),
            raw_vec(start),
            raw_vec(end),
            corridor.as_ptr(),
            path_count,
            buffers.points.as_mut_ptr(),
            buffers.flags.as_mut_ptr(),
            buffers.polygons.as_mut_ptr(),
            capacity,
            &raw mut count,
            &raw mut details,
        )
    };
    check(status)?;
    let count = checked_count(count, buffers.points.len())?;
    Ok((count, Details(details)))
}

pub(crate) fn raycast(
    query: QueryPtr,
    start_polygon: u32,
    start: Vec3A,
    end: Vec3A,
    filter: &QueryFilter,
    visited: &mut [u32],
) -> Result<(usize, Details, Raycast), Status> {
    let capacity = u32::try_from(visited.len()).map_err(|_error| Status::InvalidArgument)?;
    let filter = raw_filter(filter);
    let mut count = 0;
    let mut details = 0;
    let mut result = raw::BFNavigationRaycastResult::default();
    // SAFETY: the query is live, the wrapper validates `start_polygon`, the
    // visited buffer is writable for `capacity`, and all output records are distinct.
    let status = unsafe {
        raw::bf_navigation_query_raycast(
            query.0.as_ptr(),
            start_polygon,
            raw_vec(start),
            raw_vec(end),
            &raw const filter,
            visited.as_mut_ptr(),
            capacity,
            &raw mut count,
            &raw mut details,
            &raw mut result,
        )
    };
    check(status)?;
    let count = checked_count(count, visited.len())?;
    Ok((
        count,
        Details(details),
        Raycast {
            fraction: result.fraction,
            normal: safe_vec(result.normal),
            edge_index: result.edge_index,
            path_cost: result.path_cost,
        },
    ))
}

pub(crate) fn safe_vec(value: raw::BFNavigationVec3) -> Vec3A {
    Vec3A::new(value.x, value.y, value.z)
}

fn raw_filter(filter: &QueryFilter) -> raw::BFNavigationFilter {
    raw::BFNavigationFilter {
        include_flags: filter.include_flags,
        exclude_flags: filter.exclude_flags,
        area_costs: filter.area_costs,
    }
}

fn raw_vec(value: Vec3A) -> raw::BFNavigationVec3 {
    raw::BFNavigationVec3 {
        x: value.x,
        y: value.y,
        z: value.z,
    }
}

fn checked_count(count: u32, capacity: usize) -> Result<usize, Status> {
    let count = usize::try_from(count).map_err(|_error| Status::ContractViolation)?;
    if count <= capacity {
        Ok(count)
    } else {
        Err(Status::ContractViolation)
    }
}

fn check(status: i32) -> Result<(), Status> {
    let Ok(status) = u32::try_from(status) else {
        return Err(Status::ContractViolation);
    };
    match status {
        raw::BF_NAVIGATION_STATUS_OK => Ok(()),
        raw::BF_NAVIGATION_STATUS_INVALID_ARGUMENT => Err(Status::InvalidArgument),
        raw::BF_NAVIGATION_STATUS_OUT_OF_MEMORY => Err(Status::OutOfMemory),
        raw::BF_NAVIGATION_STATUS_INVALID_NAVMESH_DATA => Err(Status::InvalidNavMeshData),
        raw::BF_NAVIGATION_STATUS_INITIALIZATION_FAILED => Err(Status::InitializationFailed),
        raw::BF_NAVIGATION_STATUS_TILE_ALREADY_OCCUPIED => Err(Status::TileAlreadyOccupied),
        raw::BF_NAVIGATION_STATUS_QUERY_FAILED => Err(Status::QueryFailed),
        _ => Err(Status::ContractViolation),
    }
}
