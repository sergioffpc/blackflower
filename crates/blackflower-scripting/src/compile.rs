use crate::{Error, ffi};

/// Luau bytecode optimization level.
#[derive(Debug, Default, Clone, Copy, PartialEq, Eq)]
#[repr(i32)]
pub enum OptimizationLevel {
    /// Disable bytecode optimizations.
    None = 0,
    /// Apply the baseline optimizations that preserve debuggability.
    #[default]
    Baseline = 1,
    /// Apply aggressive optimizations, including inlining.
    Aggressive = 2,
}

/// Debug information emitted into Luau bytecode.
#[derive(Debug, Default, Clone, Copy, PartialEq, Eq)]
#[repr(i32)]
pub enum DebugLevel {
    /// Omit debug information.
    None = 0,
    /// Emit line information and function names.
    #[default]
    LineInfo = 1,
    /// Emit full local and upvalue information.
    Full = 2,
}

/// Type information emitted into Luau bytecode.
#[derive(Debug, Default, Clone, Copy, PartialEq, Eq)]
#[repr(i32)]
pub enum TypeInfoLevel {
    /// Emit type information only for native modules.
    #[default]
    NativeModules = 0,
    /// Emit type information for every module.
    AllModules = 1,
}

/// Coverage counters emitted into Luau bytecode.
#[derive(Debug, Default, Clone, Copy, PartialEq, Eq)]
#[repr(i32)]
pub enum CoverageLevel {
    /// Disable coverage instrumentation.
    #[default]
    None = 0,
    /// Instrument statements.
    Statements = 1,
    /// Instrument statements and expressions.
    StatementsAndExpressions = 2,
}

/// Supported options for compiling Luau source.
#[derive(Debug, Default, Clone, Copy, PartialEq, Eq)]
pub struct CompileOptions {
    /// Bytecode optimization level.
    pub optimization: OptimizationLevel,
    /// Debug information level.
    pub debug: DebugLevel,
    /// Type information level.
    pub type_info: TypeInfoLevel,
    /// Coverage instrumentation level.
    pub coverage: CoverageLevel,
}

/// Owned bytecode produced by Luau 0.731.
///
/// Bytecode is tied to the pinned Luau VM version and must be versioned with
/// cooked content rather than treated as a stable interchange format.
#[derive(Debug, Clone, PartialEq, Eq)]
pub struct Bytecode {
    bytes: Box<[u8]>,
    compile_options: CompileOptions,
}

impl Bytecode {
    pub(crate) fn from_vec(bytes: Vec<u8>, compile_options: CompileOptions) -> Self {
        Self {
            bytes: bytes.into_boxed_slice(),
            compile_options,
        }
    }

    /// Reconstruct bytecode loaded from authenticated cooked content.
    ///
    /// The bytecode version and structure are validated when a [`crate::Runtime`]
    /// loads the chunk.
    #[must_use]
    pub fn from_bytes(bytes: impl Into<Box<[u8]>>) -> Self {
        Self::from_bytes_with_options(bytes, CompileOptions::default())
    }

    /// Reconstruct cooked bytecode with its authenticated compiler options.
    ///
    /// The runtime uses [`CompileOptions::type_info`] to select whether native
    /// codegen is restricted to `--!native` modules or may compile every
    /// loaded module.
    #[must_use]
    pub fn from_bytes_with_options(
        bytes: impl Into<Box<[u8]>>,
        compile_options: CompileOptions,
    ) -> Self {
        Self {
            bytes: bytes.into(),
            compile_options,
        }
    }

    /// Return the encoded Luau bytecode.
    #[must_use]
    pub const fn as_bytes(&self) -> &[u8] {
        &self.bytes
    }

    /// Consume the wrapper and return its owned encoded bytecode.
    #[must_use]
    pub fn into_bytes(self) -> Box<[u8]> {
        self.bytes
    }

    /// Return the encoded bytecode length.
    #[must_use]
    pub const fn len(&self) -> usize {
        self.bytes.len()
    }

    /// Return whether the encoded bytecode is empty.
    #[must_use]
    pub const fn is_empty(&self) -> bool {
        self.bytes.is_empty()
    }

    /// Compiler options associated with these bytecode bytes.
    #[must_use]
    pub const fn compile_options(&self) -> CompileOptions {
        self.compile_options
    }
}

/// Compile Luau source with the selected bytecode options.
///
/// Compilation runs outside any [`crate::RuntimeConfig`] VM allocator limit.
/// Compile untrusted source in a separately constrained cooker or worker.
pub fn compile(source: &str, options: CompileOptions) -> Result<Bytecode, Error> {
    let bytes = ffi::compile(source, options)?;
    if bytes.first() == Some(&0) {
        return Err(Error::Compile(
            String::from_utf8_lossy(&bytes[1..]).into_owned(),
        ));
    }
    Ok(Bytecode::from_vec(bytes, options))
}
