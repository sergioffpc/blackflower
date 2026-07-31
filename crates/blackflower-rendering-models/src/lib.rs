//! Validated runtime representations for cooked models and static meshes.
//!
//! Authoring formats and mesh optimization remain in the asset cooker. This
//! crate owns only the deterministic binary contracts consumed by rendering.

mod error;
mod mesh;
mod model;

pub use error::Error;
pub use mesh::{
    Bounds, MeshAsset, MeshLod, MeshPrimitive, MeshVertex, VertexAttributes, encode_mesh,
};
pub use model::{
    ModelAsset, ModelAttachment, ModelAttachmentKind, ModelNode, NodeTransform, encode_model,
};
