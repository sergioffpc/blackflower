use std::env;
use std::error::Error;
use std::panic::{AssertUnwindSafe, catch_unwind};
use std::path::{Path, PathBuf};

const JOLT_VERSION: &str = "5.6.0";
const NATIVE_BUILD: &str = "native/CMakeLists.txt";
const WRAPPER_HEADER: &str = "native/wrapper.h";
const WRAPPER_SOURCE: &str = "native/wrapper.cpp";

fn main() -> Result<(), Box<dyn Error>> {
    for path in [NATIVE_BUILD, WRAPPER_HEADER, WRAPPER_SOURCE] {
        println!("cargo:rerun-if-changed={path}");
        require_file(path)?;
    }
    blackflower_build::emit_rerun_environment();
    let manifest_dir =
        PathBuf::from(env::var_os("CARGO_MANIFEST_DIR").ok_or("CARGO_MANIFEST_DIR is not set")?);
    let (configuration, workspace_root, jolt) =
        blackflower_build::locate_from_cargo_build_script(&manifest_dir, "jolt", JOLT_VERSION)
            .map_err(blackflower_build_error)?;
    let jolt_source = workspace_root.join("vendor/JoltPhysics");
    let jolt_library =
        blackflower_build::find_static_library(&jolt, &configuration, "Jolt", "Jolt")
            .map_err(blackflower_build_error)?;

    let install_dir = compile_wrapper(&configuration, &jolt_source, &jolt_library);
    generate_bindings()?;
    link_native(&install_dir, &jolt_library)?;

    Ok(())
}

fn require_file(path: &str) -> Result<(), Box<dyn Error>> {
    if Path::new(path).is_file() {
        return Ok(());
    }

    Err(format!(
        "missing {path}; initialize the Jolt Physics submodule with \
         `git submodule update --init --recursive`"
    )
    .into())
}

fn compile_wrapper(
    configuration: &blackflower_build::Configuration,
    jolt_source: &Path,
    jolt_library: &Path,
) -> PathBuf {
    let mut config = cmake::Config::new("native");
    config
        .profile("Distribution")
        .static_crt(configuration.crt_static)
        .define("BLACKFLOWER_JOLT_ROOT", jolt_source)
        .define("BLACKFLOWER_JOLT_LIBRARY", jolt_library);
    config.build()
}

fn generate_bindings() -> Result<(), Box<dyn Error>> {
    let out_dir = PathBuf::from(env::var_os("OUT_DIR").ok_or("OUT_DIR is not set")?);
    let builder = bindgen::Builder::default()
        .header(WRAPPER_HEADER)
        .allowlist_function("^bf_physics_.*")
        .allowlist_type("^BFPhysics.*")
        .allowlist_var("^BF_PHYSICS_.*")
        .derive_default(true)
        .generate_comments(false)
        .layout_tests(false)
        .parse_callbacks(Box::new(bindgen::CargoCallbacks::new()));
    let generation = catch_unwind(AssertUnwindSafe(|| builder.generate())).map_err(|_payload| {
        "failed to load libclang for Jolt bindings; install libclang and set \
         LIBCLANG_PATH to the directory containing the shared library"
    })?;
    let bindings = generation.map_err(|error| {
        format!(
            "failed to generate Jolt bindings; install libclang and set \
             LIBCLANG_PATH if it is not discoverable: {error}"
        )
    })?;

    bindings.write_to_file(out_dir.join("jolt_bindings.rs"))?;
    Ok(())
}

fn link_native(install_dir: &Path, jolt_library: &Path) -> Result<(), Box<dyn Error>> {
    for directory in ["lib", "lib64"] {
        let path = install_dir.join(directory);
        if path.is_dir() {
            println!("cargo:rustc-link-search=native={}", path.display());
        }
    }
    println!("cargo:rustc-link-lib=static=blackflower_physics_wrapper");
    blackflower_build::emit_static_library(jolt_library).map_err(blackflower_build_error)?;

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
