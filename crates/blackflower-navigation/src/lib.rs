#![doc = include_str!("../README.md")]

mod asset;
mod error;
mod ffi;
mod filter;
mod navmesh;
mod query;

pub use asset::{
    NAVIGATION_ASSET_SCHEMA, NavAgentProfile, NavAgentProfileId, NavMeshAsset, NavigationArea,
    NavigationAreaKey, NavigationBuildSettings, NavigationTile,
};
pub use error::Error;
pub use filter::{MAX_AREAS, QueryFilter};
pub use navmesh::{NavMesh, NavMeshParams, PolygonRef, TileRef};
pub use query::{NearestPoint, Path, PathPoint, PathPointKind, Query, QueryBuilder, RaycastHit};

/// The RecastNavigation version compiled into this crate.
#[must_use]
pub fn recastnavigation_version() -> (u32, u32, u32) {
    ffi::recastnavigation_version()
}

/// The Detour tile-data version accepted by this runtime.
#[must_use]
pub fn detour_navmesh_version() -> u32 {
    ffi::detour_navmesh_version()
}
