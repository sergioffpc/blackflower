/// Errors produced by offline glTF to Recast/Detour navigation cooking.
#[derive(Debug, thiserror::Error)]
pub enum Error {
    #[error("failed to parse or import navigation glTF source")]
    Gltf(#[source] gltf::Error),
    #[error("invalid Blackflower metadata in navigation source")]
    Metadata(#[source] blackflower_gltf_metadata::Error),
    #[error("invalid navigation source: {0}")]
    InvalidSource(String),
    #[error("invalid navigation area policy: {0}")]
    InvalidArea(String),
    #[error("native Recast cooker rejected the source: {0}")]
    Native(String),
    #[error("native Recast cooker allocation failed")]
    Allocation,
    #[error("cooked navigation output violates the runtime contract")]
    InvalidOutput(#[source] blackflower_navigation::Error),
}
