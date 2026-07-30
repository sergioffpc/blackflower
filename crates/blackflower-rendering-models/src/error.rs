/// Errors produced while encoding or loading a cooked model asset.
#[derive(Debug, thiserror::Error)]
pub enum Error {
    /// Source mesh data violates the runtime model contract.
    #[error("invalid model input: {0}")]
    InvalidInput(String),
    /// Authenticated bytes are not a supported Blackflower model asset.
    #[error("invalid model asset: {0}")]
    InvalidAsset(String),
}
