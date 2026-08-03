use bytes::Bytes;

use crate::{Error, ffi};

const SPIRV_MAGIC: [u8; 4] = 0x0723_0203_u32.to_le_bytes();

/// Shader stage assigned to one Slang entry point.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
#[repr(i32)]
pub enum ShaderStage {
    /// Vertex-processing entry point.
    Vertex = 1,
    /// Fragment-processing entry point.
    Fragment = 2,
    /// Compute-processing entry point.
    Compute = 3,
}

/// Slang optimization level used during SPIR-V generation.
#[derive(Debug, Default, Clone, Copy, PartialEq, Eq)]
#[repr(i32)]
pub enum OptimizationLevel {
    /// Disable optimizations.
    None = 0,
    /// Balance generated code quality and compilation time.
    #[default]
    Default = 1,
    /// Optimize aggressively.
    High = 2,
    /// Permit expensive optimization and size-versus-speed trade-offs.
    Maximal = 3,
}

/// Debug information emitted into SPIR-V.
#[derive(Debug, Default, Clone, Copy, PartialEq, Eq)]
#[repr(i32)]
pub enum DebugInfoLevel {
    /// Omit debug information.
    None = 0,
    /// Preserve the minimum information needed for stack traces.
    Minimal = 1,
    /// Emit the target's standard debug information.
    #[default]
    Standard = 2,
    /// Emit all available debug information.
    Maximal = 3,
}

/// Complete options for one Slang-to-SPIR-V compilation.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub struct CompileOptions {
    /// Stage of the selected entry point.
    pub stage: ShaderStage,
    /// Slang optimization level.
    pub optimization: OptimizationLevel,
    /// SPIR-V debug information level.
    pub debug_info: DebugInfoLevel,
}

/// Compiles one Slang entry point into SPIR-V 1.5.
///
/// # Errors
///
/// Returns an error when the source or entry point is invalid, Slang rejects
/// the program, or the native wrapper returns malformed output.
pub fn compile(
    source_name: &str,
    source: &str,
    entry_point: &str,
    options: CompileOptions,
) -> Result<Bytes, Error> {
    if source_name.is_empty() || source_name.as_bytes().contains(&0) {
        return Err(Error::InvalidInput(
            "source name must be non-empty and contain no NUL bytes".to_owned(),
        ));
    }
    if source.is_empty() {
        return Err(Error::InvalidInput("source is empty".to_owned()));
    }
    if source.as_bytes().contains(&0) {
        return Err(Error::InvalidInput(
            "source must contain no NUL bytes".to_owned(),
        ));
    }
    if entry_point.is_empty() || entry_point.as_bytes().contains(&0) {
        return Err(Error::InvalidInput(
            "entry point must be non-empty and contain no NUL bytes".to_owned(),
        ));
    }

    let bytes = ffi::compile(source_name, source, entry_point, options)?;
    if bytes.len() < SPIRV_MAGIC.len()
        || bytes.len() % size_of::<u32>() != 0
        || bytes[..SPIRV_MAGIC.len()] != SPIRV_MAGIC
    {
        return Err(Error::InvalidOutput(
            "output is not a little-endian SPIR-V module".to_owned(),
        ));
    }
    Ok(bytes)
}

/// Returns the exact pinned Slang release used by this crate.
#[must_use]
pub fn slang_version() -> &'static str {
    ffi::slang_version()
}

#[cfg(test)]
#[path = "../tests/unit/compile.rs"]
mod tests;
