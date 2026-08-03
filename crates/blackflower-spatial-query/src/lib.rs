#![doc = include_str!("../README.md")]

mod error;
mod ffi;
mod scene;
mod types;

pub use error::Error;
pub use glam::Vec3A;
pub use scene::{Device, Scene, SceneBuilder};
pub use types::{GeometryId, InstanceId, PrimitiveId, SurfaceHit, Triangle};

/// Embree version pinned by the shared native-vendor build.
pub const EMBREE_VERSION: (u32, u32, u32) = (4, 4, 1);

/// Return the linked Embree version reported by the native wrapper.
#[must_use]
pub fn embree_version() -> (u32, u32, u32) {
    ffi::embree_version()
}
