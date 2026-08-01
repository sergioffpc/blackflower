use std::env;
use std::error::Error;
use std::panic::{AssertUnwindSafe, catch_unwind};
use std::path::{Path, PathBuf};

const NATIVE_BUILD: &str = "native/CMakeLists.txt";
const OZZ_VERSION: &str = "0.16.0";
const WRAPPER_HEADER: &str = "native/wrapper.h";
const WRAPPER_SOURCE: &str = "native/wrapper.cpp";

fn main() -> Result<(), Box<dyn Error>> {
    for path in [NATIVE_BUILD, WRAPPER_HEADER, WRAPPER_SOURCE] {
        println!("cargo:rerun-if-changed={path}");
        require_file(path)?;
    }
    blackflower_build::emit_cargo_directives();
    let manifest_dir =
        PathBuf::from(env::var_os("CARGO_MANIFEST_DIR").ok_or("CARGO_MANIFEST_DIR is not set")?);
    let (configuration, workspace_root, ozz) =
        blackflower_build::locate_from_cargo_build_script(&manifest_dir, "ozz", OZZ_VERSION)
            .map_err(blackflower_build_error)?;
    let ozz_source = workspace_root.join("vendor/ozz-animation");
    let animation = blackflower_build::find_static_library(
        &ozz,
        &configuration,
        "ozz_animation",
        "ozz_animation",
    )
    .map_err(blackflower_build_error)?;
    let base = blackflower_build::find_static_library(&ozz, &configuration, "ozz_base", "ozz_base")
        .map_err(blackflower_build_error)?;

    let install_dir = compile_wrapper(&configuration, &ozz_source, &animation, &base);
    generate_bindings()?;
    link_native(&install_dir, &animation, &base)?;
    Ok(())
}

fn require_file(path: &str) -> Result<(), Box<dyn Error>> {
    if Path::new(path).is_file() {
        return Ok(());
    }

    Err(format!(
        "missing {path}; initialize the ozz-animation submodule with \
         `git submodule update --init --recursive`"
    )
    .into())
}

fn compile_wrapper(
    configuration: &blackflower_build::Configuration,
    ozz_source: &Path,
    animation: &Path,
    base: &Path,
) -> PathBuf {
    let mut config = cmake::Config::new("native");
    config
        .profile(configuration.cmake_profile)
        .static_crt(configuration.crt_static)
        .define("BLACKFLOWER_OZZ_ROOT", ozz_source)
        .define("BLACKFLOWER_OZZ_ANIMATION_LIBRARY", animation)
        .define("BLACKFLOWER_OZZ_BASE_LIBRARY", base)
        .define("BLACKFLOWER_OZZ_VERSION_MAJOR", "0")
        .define("BLACKFLOWER_OZZ_VERSION_MINOR", "16")
        .define("BLACKFLOWER_OZZ_VERSION_PATCH", "0");
    config.build()
}

fn generate_bindings() -> Result<(), Box<dyn Error>> {
    let out_dir = PathBuf::from(env::var_os("OUT_DIR").ok_or("OUT_DIR is not set")?);
    let builder = bindgen::Builder::default()
        .header(WRAPPER_HEADER)
        .allowlist_function("^bf_animation_.*")
        .allowlist_type("^BFAnimation.*")
        .allowlist_var("^BF_ANIMATION_.*")
        .derive_default(true)
        .generate_comments(false)
        .layout_tests(false)
        .parse_callbacks(Box::new(bindgen::CargoCallbacks::new()));
    let generation = catch_unwind(AssertUnwindSafe(|| builder.generate())).map_err(|_payload| {
        "failed to load libclang for ozz-animation bindings; install libclang and set \
         LIBCLANG_PATH to the directory containing the shared library"
    })?;
    let bindings = generation.map_err(|error| {
        format!(
            "failed to generate ozz-animation bindings; install libclang and set \
             LIBCLANG_PATH if it is not discoverable: {error}"
        )
    })?;

    bindings.write_to_file(out_dir.join("ozz_bindings.rs"))?;
    Ok(())
}

fn link_native(install_dir: &Path, animation: &Path, base: &Path) -> Result<(), Box<dyn Error>> {
    for directory in ["lib", "lib64"] {
        let path = install_dir.join(directory);
        if path.is_dir() {
            println!("cargo:rustc-link-search=native={}", path.display());
        }
    }
    println!("cargo:rustc-link-lib=static=blackflower_animation_wrapper");
    blackflower_build::emit_static_library(animation).map_err(blackflower_build_error)?;
    blackflower_build::emit_static_library(base).map_err(blackflower_build_error)?;

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

fn blackflower_build_error(error: Box<dyn Error + Send + Sync>) -> std::io::Error {
    std::io::Error::other(error.to_string())
}
