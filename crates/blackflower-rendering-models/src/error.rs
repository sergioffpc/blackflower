/// Errors produced while encoding or loading a cooked model or mesh asset.
#[derive(Debug, thiserror::Error)]
pub enum Error {
    /// Source data violates a runtime model or mesh contract.
    #[error("invalid rendering model input: {0}")]
    InvalidInput(String),
    /// Authenticated bytes are not a supported Blackflower model or mesh.
    #[error("invalid rendering model asset: {0}")]
    InvalidAsset(String),
}
