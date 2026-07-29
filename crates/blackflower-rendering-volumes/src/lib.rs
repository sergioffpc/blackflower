#![doc = include_str!("../README.md")]

mod error;
mod ffi;
mod types;
mod vdb;

pub use error::Error;
pub use types::{Bounds3, FloatVoxel, GridClass, GridMetadata, GridType, IndexBounds, WorldBounds};
pub use vdb::{FloatGrid, Grid, GridIter, Vdb};

/// The OpenVDB release that supplies the native VDB implementation.
#[must_use]
pub fn openvdb_version() -> (u32, u32, u32) {
    ffi::openvdb_version()
}

/// The VDB binary-format version compiled into this crate.
#[must_use]
pub fn vdb_version() -> (u32, u32, u32) {
    ffi::vdb_version()
}
