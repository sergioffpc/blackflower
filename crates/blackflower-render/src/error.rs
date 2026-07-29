/// Errors produced while loading or sampling NanoVDB assets.
#[derive(Debug, Clone, PartialEq, Eq, thiserror::Error)]
pub enum Error {
    /// The bytes are not a compatible NanoVDB 32.x asset.
    #[error("invalid or incompatible NanoVDB asset")]
    InvalidAsset,
    /// The asset uses ZIP or BLOSC compression, which is not enabled at runtime.
    #[error("compressed NanoVDB assets are unsupported; cook an uncompressed .nvdb asset")]
    UnsupportedCompression,
    /// The native NanoVDB runtime could not allocate the asset.
    #[error("NanoVDB native allocation failed")]
    OutOfMemory,
    /// A grid name is not valid UTF-8.
    #[error("NanoVDB grid name is not valid UTF-8")]
    InvalidGridName,
    /// A world- or index-space position contains a non-finite component.
    #[error("NanoVDB position components must be finite")]
    InvalidPosition,
    /// The private native wrapper rejected an internal call.
    #[error("native NanoVDB wrapper contract violation")]
    NativeContract,
}
