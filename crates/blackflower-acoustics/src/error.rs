/// Authoritative acoustic asset or runtime failure.
#[derive(Debug, thiserror::Error)]
pub enum Error {
    /// An asset container is truncated, corrupt, non-canonical, or unsupported.
    #[error("invalid {format}: {reason}")]
    InvalidAsset {
        /// Short format name.
        format: &'static str,
        /// Stable validation reason.
        reason: &'static str,
    },
    /// A public constructor received invalid quantized data.
    #[error("invalid acoustic field `{0}`")]
    InvalidField(&'static str),
    /// A stable identifier occurs more than once.
    #[error("duplicate acoustic identifier `{0}`")]
    DuplicateIdentifier(String),
    /// An asset references an identifier absent from its dependency.
    #[error("missing acoustic reference `{0}`")]
    MissingReference(String),
    /// A configured fixed-capacity pool is full.
    #[error("acoustic resource limit reached for {0}")]
    ResourceLimit(&'static str),
}
