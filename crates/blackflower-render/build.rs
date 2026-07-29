use std::env;
use std::error::Error;
use std::fs;
use std::panic::{AssertUnwindSafe, catch_unwind};
use std::path::{Path, PathBuf};

const NATIVE_BUILD: &str = "native/CMakeLists.txt";
const NANOVDB_HEADER: &str = "vendor/openvdb/nanovdb/nanovdb/NanoVDB.h";
const OPENVDB_BUILD: &str = "vendor/openvdb/CMakeLists.txt";
const WRAPPER_HEADER: &str = "native/wrapper.h";
const WRAPPER_SOURCE: &str = "native/wrapper.cpp";

fn main() -> Result<(), Box<dyn Error>> {
    for path in [
        NATIVE_BUILD,
        NANOVDB_HEADER,
        OPENVDB_BUILD,
        WRAPPER_HEADER,
        WRAPPER_SOURCE,
    ] {
        println!("cargo:rerun-if-changed={path}");
        require_file(path)?;
    }
    println!("cargo:rerun-if-changed=vendor/openvdb/nanovdb/nanovdb");

    let version = read_openvdb_version()?;
    let install_dir = compile_native(version)?;
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

fn read_openvdb_version() -> Result<[u32; 3], Box<dyn Error>> {
    let source = fs::read_to_string(OPENVDB_BUILD)?;
    Ok([
        version_component(&source, "MAJOR")?,
        version_component(&source, "MINOR")?,
        version_component(&source, "PATCH")?,
    ])
}

fn version_component(source: &str, component: &str) -> Result<u32, Box<dyn Error>> {
    let prefix = format!("set(OpenVDB_{component}_VERSION ");
    let value = source
        .lines()
        .find_map(|line| line.trim().strip_prefix(&prefix))
        .and_then(|value| value.strip_suffix(')'))
        .ok_or_else(|| format!("OpenVDB does not declare OpenVDB_{component}_VERSION"))?;
    Ok(value.parse()?)
}

fn compile_native(version: [u32; 3]) -> Result<PathBuf, Box<dyn Error>> {
    let mut config = cmake::Config::new("native");
    config
        .profile("Release")
        .define("BLACKFLOWER_OPENVDB_VERSION_MAJOR", version[0].to_string())
        .define("BLACKFLOWER_OPENVDB_VERSION_MINOR", version[1].to_string())
        .define("BLACKFLOWER_OPENVDB_VERSION_PATCH", version[2].to_string());
    Ok(config.build())
}

fn generate_bindings() -> Result<(), Box<dyn Error>> {
    let out_dir = PathBuf::from(env::var_os("OUT_DIR").ok_or("OUT_DIR is not set")?);
    let builder = bindgen::Builder::default()
        .header(WRAPPER_HEADER)
        .allowlist_function("^bf_render_.*")
        .allowlist_type("^BFRenderNanoVdb.*")
        .allowlist_var("^BF_RENDER_NANOVDB_.*")
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
    println!("cargo:rustc-link-lib=static=blackflower_render_nanovdb_wrapper");

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
