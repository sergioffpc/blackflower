#![allow(
    unsafe_code,
    unsafe_op_in_unsafe_fn,
    reason = "raw Recast cooker calls are isolated in this private module"
)]
#![allow(
    clippy::undocumented_unsafe_blocks,
    clippy::multiple_unsafe_ops_per_block,
    reason = "all unsafe operations are confined to the reviewed cooker FFI boundary"
)]

use std::num::NonZeroU32;
use std::ptr::NonNull;

use blackflower_navigation::{
    Error as NavigationError, NavAgentProfile, NavMeshParams, NavigationBuildSettings,
    NavigationTile,
};
use bytes::Bytes;
use glam::Vec3A;

use crate::Error;
use crate::geometry::{Geometry, NativeAreas};

#[allow(
    dead_code,
    non_camel_case_types,
    non_snake_case,
    non_upper_case_globals,
    unsafe_code,
    reason = "generated declarations mirror the Blackflower Recast cooker wrapper"
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
mod raw {
    include!(concat!(env!("OUT_DIR"), "/navigation_cooker_bindings.rs"));
}

pub(crate) struct CookedNative {
    pub(crate) params: NavMeshParams,
    pub(crate) tiles: Vec<NavigationTile>,
}

#[allow(
    clippy::too_many_lines,
    reason = "the FFI call constructs one explicit native input record and immediately validates its output"
)]
pub(crate) fn cook(
    geometry: &Geometry,
    native_areas: &NativeAreas,
    agent: &NavAgentProfile,
    build: &NavigationBuildSettings,
) -> Result<CookedNative, Error> {
    let settings = raw::BFNavigationCookSettings {
        cell_size: build.cell_size(),
        cell_height: build.cell_height(),
        tile_size: build.tile_size(),
        region_min_area: build.region_min_area(),
        region_merge_area: build.region_merge_area(),
        max_edge_length: build.max_edge_length(),
        max_simplification_error: build.max_simplification_error(),
        max_vertices_per_polygon: build.max_vertices_per_polygon(),
        detail_sample_distance: build.detail_sample_distance(),
        detail_sample_max_error: build.detail_sample_max_error(),
        agent_height: agent.height(),
        agent_radius: agent.radius(),
        agent_max_climb: agent.max_climb(),
        agent_max_slope_degrees: agent.max_slope_degrees(),
    };
    let input = raw::BFNavigationCookInput {
        vertices: geometry.vertices.as_ptr(),
        vertex_count: u32::try_from(geometry.vertices.len() / 3)
            .map_err(|_error| Error::InvalidSource("vertex count exceeds u32".to_owned()))?,
        indices: geometry.indices.as_ptr(),
        triangle_areas: native_areas.triangle_areas.as_ptr(),
        triangle_count: u32::try_from(geometry.indices.len() / 3)
            .map_err(|_error| Error::InvalidSource("triangle count exceeds u32".to_owned()))?,
        area_remap: native_areas.remap.as_ptr(),
        area_traversable: native_areas.traversable.as_ptr(),
        off_mesh_vertices: pointer_or_null(&geometry.off_mesh_vertices),
        off_mesh_radii: pointer_or_null(&geometry.off_mesh_radii),
        off_mesh_directions: pointer_or_null(&geometry.off_mesh_directions),
        off_mesh_areas: pointer_or_null(&geometry.off_mesh_areas),
        off_mesh_flags: pointer_or_null(&geometry.off_mesh_flags),
        off_mesh_user_ids: pointer_or_null(&geometry.off_mesh_user_ids),
        off_mesh_count: u32::try_from(geometry.off_mesh_radii.len())
            .map_err(|_error| Error::InvalidSource("off-mesh link count exceeds u32".to_owned()))?,
    };
    let mut output = raw::BFNavigationCookOutput::default();
    let mut error = [0_i8; 512];
    let status = unsafe {
        raw::bf_navigation_cooker_build(
            &raw const settings,
            &raw const input,
            &raw mut output,
            error.as_mut_ptr(),
            error.len(),
        )
    };
    let output = Output::new(output);
    if status != raw::BF_NAVIGATION_COOK_OK.cast_signed() {
        let message = error_message(&error);
        return if status == raw::BF_NAVIGATION_COOK_OUT_OF_MEMORY.cast_signed() {
            Err(Error::Allocation)
        } else {
            Err(Error::Native(message))
        };
    }
    output.decode()
}

fn pointer_or_null<T>(values: &[T]) -> *const T {
    if values.is_empty() {
        std::ptr::null()
    } else {
        values.as_ptr()
    }
}

fn error_message(bytes: &[i8]) -> String {
    unsafe { std::ffi::CStr::from_ptr(bytes.as_ptr()) }
        .to_string_lossy()
        .into_owned()
}

struct Output(raw::BFNavigationCookOutput);

impl Output {
    const fn new(output: raw::BFNavigationCookOutput) -> Self {
        Self(output)
    }

    fn decode(&self) -> Result<CookedNative, Error> {
        let max_tiles = NonZeroU32::new(self.0.max_tiles)
            .ok_or(NavigationError::InvalidNavMeshParameters)
            .map_err(Error::InvalidOutput)?;
        let max_polygons = NonZeroU32::new(self.0.max_polygons_per_tile)
            .ok_or(NavigationError::InvalidNavMeshParameters)
            .map_err(Error::InvalidOutput)?;
        let params = NavMeshParams::new(
            Vec3A::from(self.0.origin),
            self.0.tile_width,
            self.0.tile_height,
            max_tiles,
            max_polygons,
        )
        .map_err(Error::InvalidOutput)?;
        let count = usize::try_from(self.0.tile_count)
            .map_err(|_error| Error::Native("tile count does not fit usize".to_owned()))?;
        let pointer = NonNull::new(self.0.tiles)
            .ok_or_else(|| Error::Native("native cooker returned no tile table".to_owned()))?;
        let source = unsafe { std::slice::from_raw_parts(pointer.as_ptr(), count) };
        let mut tiles = Vec::with_capacity(count);
        for tile in source {
            let length = usize::try_from(tile.data_size)
                .map_err(|_error| Error::Native("tile length does not fit usize".to_owned()))?;
            let data = NonNull::new(tile.data)
                .ok_or_else(|| Error::Native("native cooker returned a null tile".to_owned()))?;
            let bytes = unsafe { std::slice::from_raw_parts(data.as_ptr(), length) };
            tiles.push(
                NavigationTile::new(tile.x, tile.y, tile.layer, Bytes::copy_from_slice(bytes))
                    .map_err(Error::InvalidOutput)?,
            );
        }
        Ok(CookedNative { params, tiles })
    }
}

impl Drop for Output {
    fn drop(&mut self) {
        unsafe { raw::bf_navigation_cooker_free(&raw mut self.0) };
    }
}
