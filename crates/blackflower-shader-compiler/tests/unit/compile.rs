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
