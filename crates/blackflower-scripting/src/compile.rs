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
}

impl Bytecode {
    pub(crate) fn from_vec(bytes: Vec<u8>) -> Self {
        Self {
            bytes: bytes.into_boxed_slice(),
        }
    }

    /// Return the encoded Luau bytecode.
    #[must_use]
    pub const fn as_bytes(&self) -> &[u8] {
        &self.bytes
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
}

/// Compile Luau source with the selected bytecode options.
///
/// Luau encodes syntax errors into the returned bytecode. They are reported as
/// [`Error::Compile`] when a [`crate::Runtime`] loads that bytecode.
///
/// Compilation runs outside any [`crate::RuntimeConfig`] VM allocator limit.
/// Compile untrusted source in a separately constrained cooker or worker.
pub fn compile(source: &str, options: CompileOptions) -> Result<Bytecode, Error> {
    ffi::compile(source, options).map(Bytecode::from_vec)
}
