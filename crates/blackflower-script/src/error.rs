/// Errors produced while compiling or executing Luau.
#[derive(Debug, Clone, PartialEq, Eq, thiserror::Error)]
pub enum Error {
    /// A runtime was configured without any VM memory.
    #[error("Luau VM memory limit must be greater than zero")]
    InvalidMemoryLimit,
    /// A runtime was configured without any execution fuel.
    #[error("Luau execution fuel must be greater than zero")]
    InvalidExecutionFuel,
    /// The native compiler or VM could not allocate memory.
    #[error("Luau allocation failed")]
    OutOfMemory,
    /// A chunk exhausted its configured VM safepoint fuel.
    #[error("Luau execution fuel exhausted")]
    ExecutionLimit,
    /// The VM could not initialize its deterministic standard library.
    #[error("Luau runtime initialization failed: {0}")]
    Initialization(String),
    /// Luau rejected source or bytecode while loading a chunk.
    #[error("Luau compilation failed: {0}")]
    Compile(String),
    /// The native Luau compiler failed without producing bytecode.
    #[error("native Luau compiler failed")]
    CompilerFailure,
    /// A protected Luau call failed while executing a chunk.
    #[error("Luau execution failed: {0}")]
    Runtime(String),
    /// A chunk name contains a nul byte and cannot cross the native API.
    #[error("Luau chunk names cannot contain nul bytes")]
    InvalidChunkName,
    /// A returned Luau value is outside the initial safe value surface.
    #[error("unsupported Luau result at index {index}: {type_name}")]
    UnsupportedValue {
        /// Zero-based result index.
        index: usize,
        /// Luau's stable type name.
        type_name: String,
    },
    /// The native wrapper violated an internal ownership or stack contract.
    #[error("native Luau wrapper contract violation")]
    NativeContract,
}
