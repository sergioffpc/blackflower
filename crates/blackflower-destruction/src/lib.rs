#![doc = include_str!("../README.md")]

mod asset;
mod error;
mod family;
mod ffi;
mod types;

pub use asset::Asset;
pub use error::Error;
pub use family::Family;
pub use types::{
    ActorId, BondDesc, ChunkDesc, ForceMode, FractureCommand, FractureEvent, GraphNodeId,
    StressSettings, StressStats,
};

/// The NVIDIA Blast version compiled into this crate.
#[must_use]
pub fn blast_version() -> &'static str {
    ffi::blast_version()
}

/// Whether the pinned upstream `NvBlastExtStress` implementation supports the target.
#[must_use]
pub fn stress_supported() -> bool {
    ffi::stress_supported()
}
