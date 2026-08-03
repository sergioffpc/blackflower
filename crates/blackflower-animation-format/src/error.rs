/// Errors produced while encoding or decoding Blackflower animation assets.
#[derive(Debug, Clone, PartialEq, Eq, thiserror::Error)]
pub enum Error {
    /// The file does not begin with the expected typed magic.
    #[error("invalid Blackflower animation asset magic")]
    InvalidMagic,
    /// The file uses a container schema this implementation does not support.
    #[error("unsupported Blackflower animation container schema {0}")]
    UnsupportedSchema(u16),
    /// The fixed header or section table is malformed.
    #[error("invalid Blackflower animation container header")]
    InvalidHeader,
    /// A section descriptor or its byte range is invalid.
    #[error("invalid Blackflower animation container section")]
    InvalidSection,
    /// Clip metadata is malformed.
    #[error("invalid Blackflower animation clip metadata")]
    InvalidClipMetadata,
    /// A clip or marker name is invalid.
    #[error("invalid Blackflower animation metadata name")]
    InvalidName,
    /// A marker ratio is invalid.
    #[error("animation marker ratio must be finite and in 0..=1")]
    InvalidMarkerRatio,
    /// Markers are not in deterministic timeline order.
    #[error("animation markers are not in deterministic order")]
    InvalidMarkerOrder,
    /// Two markers have the same name and exact normalized time.
    #[error("animation contains a duplicate marker")]
    DuplicateMarker,
    /// A rig joint definition is malformed.
    #[error("invalid skeleton joint definition")]
    InvalidRig,
    /// A container length cannot be represented by the format.
    #[error("Blackflower animation asset is too large")]
    AssetTooLarge,
}
