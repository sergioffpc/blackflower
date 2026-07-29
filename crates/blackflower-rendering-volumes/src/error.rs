/// Errors produced while loading or sampling VDB assets.
#[derive(Debug, Clone, PartialEq, Eq, thiserror::Error)]
pub enum Error {
    /// The bytes are not a compatible VDB 32.x asset.
    #[error("invalid or incompatible VDB asset")]
    InvalidAsset,
    /// The asset uses ZIP or BLOSC compression, which is not enabled at runtime.
    #[error("compressed VDB assets are unsupported; cook an uncompressed .nvdb asset")]
    UnsupportedCompression,
    /// The native VDB runtime could not allocate the asset.
    #[error("VDB native allocation failed")]
    OutOfMemory,
    /// A grid name is not valid UTF-8.
    #[error("VDB grid name is not valid UTF-8")]
    InvalidGridName,
    /// A world- or index-space position contains a non-finite component.
    #[error("VDB position components must be finite")]
    InvalidPosition,
    /// The private native wrapper rejected an internal call.
    #[error("native VDB wrapper contract violation")]
    NativeContract,
}
