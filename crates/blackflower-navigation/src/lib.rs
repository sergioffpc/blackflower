#![doc = include_str!("../README.md")]

mod error;
mod ffi;
mod filter;
mod navmesh;
mod query;

pub use error::Error;
pub use filter::{MAX_AREAS, QueryFilter};
pub use navmesh::{NavMesh, NavMeshParams, PolygonRef, TileRef};
pub use query::{NearestPoint, Path, PathPoint, PathPointKind, Query, QueryBuilder, RaycastHit};

/// The RecastNavigation version compiled into this crate.
#[must_use]
pub fn recastnavigation_version() -> (u32, u32, u32) {
    ffi::recastnavigation_version()
}
