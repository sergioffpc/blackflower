/// Errors produced while compiling or executing Luau.
#[derive(Debug, Clone, PartialEq, Eq, thiserror::Error)]
pub enum Error {
    /// A runtime was configured without any VM memory.
    #[error("Luau VM memory limit must be greater than zero")]
    InvalidMemoryLimit,
    /// A runtime was configured without any execution fuel.
    #[error("Luau execution fuel must be greater than zero")]
    InvalidExecutionFuel,
    /// Native codegen was enabled with an executable-memory budget below the minimum.
    #[error("Luau native codegen memory limit is too small")]
    InvalidNativeCodegenLimit,
    /// The current target cannot execute Luau native code.
    #[error("Luau native codegen is unsupported on this target")]
    NativeCodegenUnsupported,
    /// Luau could not initialize its native code generator.
    #[error("Luau native codegen initialization failed")]
    NativeCodegenInitialization,
    /// Luau could not generate native code for a loaded chunk.
    #[error("Luau native code generation failed")]
    NativeCodegenCompilation,
    /// Authenticated content has a runtime kind other than Luau bytecode.
    #[error("authenticated asset is {actual:?}, not Luau bytecode")]
    InvalidBytecodeAssetKind {
        /// Authenticated catalog kind supplied by the asset package.
        actual: blackflower_assets::AssetKind,
    },
    /// Authenticated bytecode was cooked for another Luau toolchain.
    #[error("Luau bytecode toolchain mismatch: expected `{expected}`, found `{actual}`")]
    IncompatibleBytecodeToolchain {
        /// Exact linked Luau toolchain identity.
        expected: String,
        /// Toolchain identity authenticated by the asset catalog.
        actual: String,
    },
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
    /// No executable instruction could be associated with a requested source line.
    #[error("Luau breakpoint line {line} is not executable")]
    InvalidBreakpoint {
        /// Requested one-based source line.
        line: u32,
    },
    /// A user-provided debugger handler panicked inside the native callback.
    #[error("Luau debugger handler panicked")]
    DebugHandlerPanicked,
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
