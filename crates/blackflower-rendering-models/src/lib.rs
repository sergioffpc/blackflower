//! Validated runtime representation for cooked static meshes and their LODs.
//!
//! Authoring formats and mesh optimization remain in the asset cooker. This
//! crate owns only the deterministic binary contract consumed by rendering.

mod error;
mod model;

pub use error::Error;
pub use model::{Bounds, MeshLod, MeshPrimitive, MeshVertex, ModelAsset, VertexAttributes, encode};
