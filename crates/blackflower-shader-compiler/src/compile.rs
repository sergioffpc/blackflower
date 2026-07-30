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
mod tests {
    use std::error::Error;
    use std::fs;

    use super::{
        CompileOptions, DebugInfoLevel, OptimizationLevel, ShaderStage, compile, slang_version,
    };

    const VERTEX_SHADER: &str = "
float4 main(float4 position : POSITION) : SV_Position
{
    return position;
}
";

    #[test]
    fn compiles_spirv_with_the_pinned_slang_release() -> Result<(), crate::Error> {
        let source_name = "shaders/test.slang";
        let options = CompileOptions {
            stage: ShaderStage::Vertex,
            optimization: OptimizationLevel::None,
            debug_info: DebugInfoLevel::Standard,
        };
        let spirv = compile(source_name, VERTEX_SHADER, "main", options)?;
        assert_eq!(&spirv[..4], &0x0723_0203_u32.to_le_bytes());
        assert!(
            spirv
                .windows(source_name.len())
                .any(|window| window == source_name.as_bytes())
        );
        assert_eq!(slang_version(), "2026.14.1");
        Ok(())
    }

    #[test]
    fn rejects_invalid_slang() {
        let options = CompileOptions {
            stage: ShaderStage::Fragment,
            optimization: OptimizationLevel::High,
            debug_info: DebugInfoLevel::None,
        };
        assert!(compile("invalid.slang", "not valid Slang", "main", options).is_err());
    }

    #[test]
    fn rejects_source_with_nul_bytes() {
        let options = CompileOptions {
            stage: ShaderStage::Compute,
            optimization: OptimizationLevel::Default,
            debug_info: DebugInfoLevel::Minimal,
        };
        assert!(
            compile(
                "nul.slang",
                "[numthreads(1, 1, 1)] void main() {}\0ignored",
                "main",
                options
            )
            .is_err()
        );
    }

    #[test]
    fn rejects_source_dependencies() -> Result<(), Box<dyn Error>> {
        let directory = tempfile::tempdir()?;
        let include_path = directory.path().join("dependency.slang");
        fs::write(
            &include_path,
            "float4 dependency(float4 position) { return position; }\n",
        )?;
        let portable_path = include_path
            .to_str()
            .ok_or("temporary include path is not UTF-8")?
            .replace('\\', "/");
        let source = format!(
            "#include \"{portable_path}\"\n\
             float4 main(float4 position : POSITION) : SV_Position {{ \
             return dependency(position); }}\n"
        );
        let options = CompileOptions {
            stage: ShaderStage::Vertex,
            optimization: OptimizationLevel::Default,
            debug_info: DebugInfoLevel::None,
        };
        let error = compile("dependencies/main.slang", &source, "main", options)
            .err()
            .ok_or("expected dependency rejection")?;
        assert!(error.to_string().contains("imports and includes"));
        Ok(())
    }
}
