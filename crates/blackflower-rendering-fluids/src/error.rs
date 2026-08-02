/// A renderer backend rejected or failed one Flow operation.
#[derive(Clone, Debug, Eq, PartialEq, thiserror::Error)]
pub enum BackendError {
    /// The backend intentionally does not implement an operation required by the selected Flow graph.
    #[error("Flow backend operation is unsupported: {0}")]
    Unsupported(&'static str),
    /// The backend could not create or encode the requested resource or pass.
    #[error("Flow backend rejected operation: {0}")]
    Rejected(&'static str),
    /// A callback returned an invalid zero resource identifier.
    #[error("Flow backend returned an invalid resource identifier")]
    InvalidResourceId,
    /// A mapped upload/readback slice is smaller than the Flow buffer descriptor.
    #[error("Flow backend returned a mapped buffer with insufficient capacity")]
    MappedBufferTooSmall,
    /// A backend callback panicked; the panic was contained at the C ABI boundary.
    #[error("Flow backend callback panicked")]
    Panicked,
}

/// Errors produced while owning or flushing an optimized Flow context.
#[derive(Clone, Debug, Eq, PartialEq, thiserror::Error)]
pub enum Error {
    /// Native context allocation failed.
    #[error("NVIDIA Flow context allocation failed")]
    AllocationFailed,
    /// A public context argument was invalid.
    #[error("invalid NVIDIA Flow context argument")]
    InvalidArgument,
    /// A renderer backend callback failed.
    #[error(transparent)]
    Backend(#[from] BackendError),
    /// The private wrapper returned an unexpected status or shape.
    #[error("native NVIDIA Flow wrapper contract violation")]
    NativeContract,
}
