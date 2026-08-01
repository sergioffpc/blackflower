#[allow(
    dead_code,
    reason = "the shared module exposes both producer and consumer halves of the native contract"
)]
#[path = "../../tools/native/support/native_vendors.rs"]
mod native_vendors;

use std::env;
use std::error::Error;
use std::panic::{AssertUnwindSafe, catch_unwind};
use std::path::{Path, PathBuf};

const NATIVE_BUILD: &str = "native/CMakeLists.txt";
const OPENVDB_VERSION: &str = "13.0.0";
const WRAPPER_HEADER: &str = "native/wrapper.h";
const WRAPPER_SOURCE: &str = "native/wrapper.cpp";

fn main() -> Result<(), Box<dyn Error>> {
    println!("cargo:rerun-if-changed=../../tools/native/support/native_vendors.rs");
    for path in [NATIVE_BUILD, WRAPPER_HEADER, WRAPPER_SOURCE] {
        println!("cargo:rerun-if-changed={path}");
        require_file(path)?;
    }
    native_vendors::emit_rerun_environment();
    let manifest_dir =
        PathBuf::from(env::var_os("CARGO_MANIFEST_DIR").ok_or("CARGO_MANIFEST_DIR is not set")?);
    let (configuration, workspace_root, _openvdb) =
        native_vendors::locate_from_cargo_build_script(&manifest_dir, "openvdb", OPENVDB_VERSION)
            .map_err(native_contract_error)?;
    let openvdb_source = workspace_root.join("vendor/openvdb");

    let install_dir = compile_wrapper(&configuration, &openvdb_source);
    generate_bindings()?;
    link_native(&install_dir)?;
    Ok(())
}

fn require_file(path: &str) -> Result<(), Box<dyn Error>> {
    if Path::new(path).is_file() {
        return Ok(());
    }

    Err(format!(
        "missing {path}; initialize the OpenVDB submodule with \
         `git submodule update --init --recursive`"
    )
    .into())
}

fn compile_wrapper(
    configuration: &native_vendors::Configuration,
    openvdb_source: &Path,
) -> PathBuf {
    let mut config = cmake::Config::new("native");
    config
        .profile("Release")
        .static_crt(configuration.crt_static)
        .define("BLACKFLOWER_OPENVDB_ROOT", openvdb_source)
        .define("BLACKFLOWER_OPENVDB_VERSION_MAJOR", "13")
        .define("BLACKFLOWER_OPENVDB_VERSION_MINOR", "0")
        .define("BLACKFLOWER_OPENVDB_VERSION_PATCH", "0");
    config.build()
}

fn generate_bindings() -> Result<(), Box<dyn Error>> {
    let out_dir = PathBuf::from(env::var_os("OUT_DIR").ok_or("OUT_DIR is not set")?);
    let builder = bindgen::Builder::default()
        .header(WRAPPER_HEADER)
        .allowlist_function("^bf_rendering_volumes_.*")
        .allowlist_type("^BFRenderingVolumesNanoVdb.*")
        .allowlist_var("^BF_RENDERING_VOLUMES_NANOVDB_.*")
        .derive_default(true)
        .generate_comments(false)
        .layout_tests(false)
        .parse_callbacks(Box::new(bindgen::CargoCallbacks::new()));
    let generation = catch_unwind(AssertUnwindSafe(|| builder.generate())).map_err(|_payload| {
        "failed to load libclang for NanoVDB bindings; install libclang and set \
         LIBCLANG_PATH to the directory containing the shared library"
    })?;
    let bindings = generation.map_err(|error| {
        format!(
            "failed to generate NanoVDB bindings; install libclang and set \
             LIBCLANG_PATH if it is not discoverable: {error}"
        )
    })?;

    bindings.write_to_file(out_dir.join("nanovdb_bindings.rs"))?;
    Ok(())
}

fn link_native(install_dir: &Path) -> Result<(), Box<dyn Error>> {
    for directory in ["lib", "lib64"] {
        let path = install_dir.join(directory);
        if path.is_dir() {
            println!("cargo:rustc-link-search=native={}", path.display());
        }
    }
    println!("cargo:rustc-link-lib=static=blackflower_rendering_volumes_nanovdb_wrapper");

    let target_os = env::var("CARGO_CFG_TARGET_OS")?;
    let target_env = env::var("CARGO_CFG_TARGET_ENV")?;
    match target_os.as_str() {
        "linux" | "android" => println!("cargo:rustc-link-lib=stdc++"),
        "macos" | "ios" | "freebsd" => println!("cargo:rustc-link-lib=c++"),
        "windows" if target_env == "gnu" => println!("cargo:rustc-link-lib=stdc++"),
        _ => {}
    }
    Ok(())
}

fn native_contract_error(error: Box<dyn Error + Send + Sync>) -> std::io::Error {
    std::io::Error::other(error.to_string())
}
