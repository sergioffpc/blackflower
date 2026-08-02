use std::env;
use std::error::Error;
use std::panic::{AssertUnwindSafe, catch_unwind};
use std::path::{Path, PathBuf};

const FLOW_VERSION: &str = "2.2.0";
const WRAPPER_HEADER: &str = "native/wrapper.h";
const WRAPPER_SOURCE: &str = "native/wrapper.cpp";
const NATIVE_BUILD: &str = "native/CMakeLists.txt";

fn main() -> Result<(), Box<dyn Error>> {
    for path in [NATIVE_BUILD, WRAPPER_HEADER, WRAPPER_SOURCE] {
        println!("cargo:rerun-if-changed={path}");
        require_file(path)?;
    }
    blackflower_build::emit_cargo_directives();
    let manifest_dir =
        PathBuf::from(env::var_os("CARGO_MANIFEST_DIR").ok_or("CARGO_MANIFEST_DIR is not set")?);
    let (configuration, workspace_root, flow) =
        blackflower_build::locate_from_cargo_build_script(&manifest_dir, "flow", FLOW_VERSION)
            .map_err(blackflower_build_error)?;
    let physx_root = workspace_root.join("vendor/PhysX");
    blackflower_build::validate_vendor_source_revision(&flow, &physx_root)
        .map_err(blackflower_build_error)?;
    let flow_library = blackflower_build::find_static_library(
        &flow,
        &configuration,
        "blackflower_flow_context_opt",
        "blackflower_flow_context_opt",
    )
    .map_err(blackflower_build_error)?;

    let install_dir = compile_wrapper(&configuration, &physx_root, &flow_library);
    generate_bindings()?;
    link_native(&install_dir, &flow_library)?;
    Ok(())
}

fn require_file(path: &str) -> Result<(), Box<dyn Error>> {
    if Path::new(path).is_file() {
        Ok(())
    } else {
        Err(format!("missing {path}").into())
    }
}

fn compile_wrapper(
    configuration: &blackflower_build::Configuration,
    physx_root: &Path,
    flow_library: &Path,
) -> PathBuf {
    let mut config = cmake::Config::new("native");
    config
        .profile(configuration.cmake_profile)
        .static_crt(configuration.crt_static)
        .define("BLACKFLOWER_PHYSX_ROOT", physx_root)
        .define("BLACKFLOWER_FLOW_LIBRARY", flow_library);
    config.build()
}

fn generate_bindings() -> Result<(), Box<dyn Error>> {
    let out_dir = PathBuf::from(env::var_os("OUT_DIR").ok_or("OUT_DIR is not set")?);
    let builder = bindgen::Builder::default()
        .header(WRAPPER_HEADER)
        .allowlist_function("^bf_flow_.*")
        .allowlist_type("^BFFlow.*")
        .allowlist_var("^BF_FLOW_.*")
        .derive_default(true)
        .generate_comments(false)
        .layout_tests(false)
        .parse_callbacks(Box::new(bindgen::CargoCallbacks::new()));
    let generation = catch_unwind(AssertUnwindSafe(|| builder.generate())).map_err(|_payload| {
        "failed to load libclang for Flow bindings; install libclang and set LIBCLANG_PATH"
    })?;
    generation
        .map_err(|error| format!("failed to generate Flow bindings: {error}"))?
        .write_to_file(out_dir.join("flow_bindings.rs"))?;
    Ok(())
}

fn link_native(install_dir: &Path, flow_library: &Path) -> Result<(), Box<dyn Error>> {
    for directory in ["lib", "lib64"] {
        let path = install_dir.join(directory);
        if path.is_dir() {
            println!("cargo:rustc-link-search=native={}", path.display());
        }
    }
    println!("cargo:rustc-link-lib=static=blackflower_flow_wrapper");
    blackflower_build::emit_static_library(flow_library).map_err(blackflower_build_error)?;

    let target_os = env::var("CARGO_CFG_TARGET_OS")?;
    let target_env = env::var("CARGO_CFG_TARGET_ENV")?;
    match target_os.as_str() {
        "linux" | "android" => {
            println!("cargo:rustc-link-lib=stdc++");
            println!("cargo:rustc-link-lib=pthread");
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
