#![doc = include_str!("../README.md")]

mod error;
mod ffi;
mod nanovdb;
mod types;

pub use error::Error;
pub use nanovdb::{FloatGrid, Grid, GridIter, NanoVdb};
pub use types::{Bounds3, FloatVoxel, GridClass, GridMetadata, GridType, IndexBounds, WorldBounds};

/// The OpenVDB release that supplies the NanoVDB headers compiled into this crate.
#[must_use]
pub fn openvdb_version() -> (u32, u32, u32) {
    ffi::openvdb_version()
}

/// The NanoVDB binary-format version compiled into this crate.
#[must_use]
pub fn nanovdb_version() -> (u32, u32, u32) {
    ffi::nanovdb_version()
}
