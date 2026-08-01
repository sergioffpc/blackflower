use std::env;
use std::error::Error;
use std::panic::{AssertUnwindSafe, catch_unwind};
use std::path::{Path, PathBuf};

const SLANG_VERSION: &str = "2026.14.1";
const NATIVE_BUILD: &str = "native/CMakeLists.txt";
const WRAPPER_HEADER: &str = "native/wrapper.h";
const WRAPPER_SOURCE: &str = "native/wrapper.cpp";

struct SlangLibraries {
    compiler: PathBuf,
    compiler_core: PathBuf,
    core: PathBuf,
    miniz: PathBuf,
    lz4: PathBuf,
    cmark: PathBuf,
}

fn main() -> Result<(), Box<dyn Error>> {
    for path in [NATIVE_BUILD, WRAPPER_HEADER, WRAPPER_SOURCE] {
        println!("cargo:rerun-if-changed={path}");
        require_file(Path::new(path))?;
    }
    blackflower_build::emit_cargo_directives();
    let manifest_dir =
        PathBuf::from(env::var_os("CARGO_MANIFEST_DIR").ok_or("CARGO_MANIFEST_DIR is not set")?);
    let (configuration, workspace_root, slang) =
        blackflower_build::locate_from_cargo_build_script(&manifest_dir, "slang", SLANG_VERSION)
            .map_err(blackflower_build_error)?;
    let slang_source = workspace_root.join("vendor/slang");
    let libraries = load_libraries(&slang, &configuration)?;

    let wrapper = compile_wrapper(&configuration, &slang_source, &libraries.compiler);
    generate_bindings()?;
    link_native(&wrapper, &libraries)?;
    Ok(())
}

fn load_libraries(
    root: &Path,
    configuration: &blackflower_build::Configuration,
) -> Result<SlangLibraries, Box<dyn Error>> {
    let find = |name: &str| {
        blackflower_build::find_static_library(root, configuration, name, name)
            .map_err(blackflower_build_error)
    };
    Ok(SlangLibraries {
        compiler: find("blackflower_slang_compiler")?,
        compiler_core: find("blackflower_slang_compiler_core")?,
        core: find("blackflower_slang_core")?,
        miniz: find("blackflower_slang_miniz")?,
        lz4: find("blackflower_slang_lz4")?,
        cmark: find("blackflower_slang_cmark_gfm")?,
    })
}

fn require_file(path: &Path) -> Result<(), Box<dyn Error>> {
    if path.is_file() {
        Ok(())
    } else {
        Err(format!("missing {}", path.display()).into())
    }
}

fn compile_wrapper(
    configuration: &blackflower_build::Configuration,
    slang_source: &Path,
    compiler: &Path,
) -> PathBuf {
    let mut config = cmake::Config::new("native");
    config
        .profile(configuration.cmake_profile)
        .static_crt(configuration.crt_static)
        .define("BLACKFLOWER_SLANG_ROOT", slang_source)
        .define("BLACKFLOWER_SLANG_LIBRARY", compiler);
    config.build()
}

fn generate_bindings() -> Result<(), Box<dyn Error>> {
    let out_dir = PathBuf::from(env::var_os("OUT_DIR").ok_or("OUT_DIR is not set")?);
    let builder = bindgen::Builder::default()
        .header(WRAPPER_HEADER)
        .allowlist_function("^bf_shader_compiler_.*")
        .allowlist_type("^BFShaderCompiler.*")
        .allowlist_var("^BF_SHADER_.*")
        .derive_default(true)
        .generate_comments(false)
        .layout_tests(false)
        .parse_callbacks(Box::new(bindgen::CargoCallbacks::new()));
    let generation = catch_unwind(AssertUnwindSafe(|| builder.generate())).map_err(|_payload| {
        "failed to load libclang for Slang bindings; install libclang and set LIBCLANG_PATH"
    })?;
    generation
        .map_err(|error| format!("failed to generate Slang bindings: {error}"))?
        .write_to_file(out_dir.join("slang_bindings.rs"))?;
    Ok(())
}

fn link_native(install_dir: &Path, libraries: &SlangLibraries) -> Result<(), Box<dyn Error>> {
    for directory in ["lib", "lib64"] {
        let path = install_dir.join(directory);
        if path.is_dir() {
            println!("cargo:rustc-link-search=native={}", path.display());
        }
    }
    println!("cargo:rustc-link-lib=static=blackflower_shader_compiler_wrapper");
    for library in [
        &libraries.compiler,
        &libraries.compiler_core,
        &libraries.core,
        &libraries.miniz,
        &libraries.lz4,
        &libraries.cmark,
    ] {
        blackflower_build::emit_static_library(library).map_err(blackflower_build_error)?;
    }

    let target_os = env::var("CARGO_CFG_TARGET_OS")?;
    let target_env = env::var("CARGO_CFG_TARGET_ENV")?;
    match target_os.as_str() {
        "linux" | "android" => {
            println!("cargo:rustc-link-lib=stdc++");
            println!("cargo:rustc-link-lib=dl");
        }
        "macos" | "ios" | "freebsd" => println!("cargo:rustc-link-lib=c++"),
        "windows" if target_env == "gnu" => println!("cargo:rustc-link-lib=stdc++"),
        _ => {}
    }
    Ok(())
}

fn blackflower_build_error(error: Box<dyn Error + Send + Sync>) -> std::io::Error {
    std::io::Error::other(error.to_string())
}
