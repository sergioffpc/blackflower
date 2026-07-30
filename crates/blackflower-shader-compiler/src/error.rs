use thiserror::Error;

/// Failure produced by the pinned Slang compiler.
#[derive(Debug, Error)]
pub enum Error {
    /// A source, entry-point, or option value cannot be represented by the native API.
    #[error("invalid shader compiler input: {0}")]
    InvalidInput(String),
    /// Slang could not initialize a compiler session.
    #[error("failed to initialize Slang compiler: {0}")]
    Initialization(String),
    /// Slang rejected the shader source or could not emit SPIR-V.
    #[error("Slang compilation failed: {0}")]
    Compilation(String),
    /// The native wrapper returned an invalid result.
    #[error("Slang compiler returned invalid output: {0}")]
    InvalidOutput(String),
}
