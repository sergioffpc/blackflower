/// Static acoustic cooking failures.
#[derive(Debug, thiserror::Error)]
pub enum Error {
    /// Typed Blackflower glTF metadata is invalid.
    #[error("invalid Blackflower glTF metadata")]
    Metadata(#[from] blackflower_gltf_metadata::Error),
    /// glTF structure or buffer data is invalid.
    #[error("invalid acoustic glTF source")]
    Gltf(#[from] gltf::Error),
    /// Authored geometry, volume, zone, or material mapping is invalid.
    #[error("{0}")]
    InvalidSource(String),
    /// Steam Audio rejected the static scene or bake.
    #[error("Steam Audio rejected acoustic cooking")]
    SteamAudio(#[from] blackflower_audio_spatial::Error),
}
