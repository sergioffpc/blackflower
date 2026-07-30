use std::path::PathBuf;

/// Errors produced by offline OpenVDB to NanoVDB cooking.
#[derive(Debug, thiserror::Error)]
pub enum Error {
    #[error("volume cooking requires at least one grid")]
    EmptyGridSelection,
    #[error("volume grid selections must be strictly sorted and unique")]
    NonCanonicalGridSelection,
    #[error("failed to create the volume cooker temporary directory")]
    TemporaryDirectory(#[source] std::io::Error),
    #[error("failed to launch the pinned volume cooker")]
    Launch(#[source] std::io::Error),
    #[error("volume cooker failed with status {status:?}: {stderr}")]
    ToolFailed { status: Option<i32>, stderr: String },
    #[error("failed to read cooked volume `{}`", path.display())]
    ReadOutput {
        path: PathBuf,
        #[source]
        source: std::io::Error,
    },
    #[error("volume cooker produced an invalid runtime VDB asset")]
    InvalidOutput(#[source] blackflower_rendering_volumes::Error),
    #[error("volume cooker output grids differ from the requested selection")]
    OutputSelectionMismatch,
}
