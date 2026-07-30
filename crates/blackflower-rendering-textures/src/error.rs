/// Errors produced while cooking, validating, or transcoding KTX2 textures.
#[derive(Debug, thiserror::Error)]
pub enum Error {
    /// Source mip dimensions, byte lengths, or options violate the texture contract.
    #[error("invalid texture input: {0}")]
    InvalidInput(String),
    /// Authenticated bytes are not a supported Blackflower KTX2 texture.
    #[error("invalid KTX2 texture: {0}")]
    InvalidKtx2(String),
    /// The requested runtime target cannot represent the texture.
    #[error("unsupported texture operation: {0}")]
    Unsupported(String),
    /// The native KTX-Software boundary failed.
    #[error("KTX-Software failed: {0}")]
    Native(String),
}
