use std::env;
use std::error::Error;
use std::panic::{AssertUnwindSafe, catch_unwind};
use std::path::{Path, PathBuf};

const BLAST_VERSION: &str = "5.0.6";
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
    let (configuration, workspace_root, blast) =
        blackflower_build::locate_from_cargo_build_script(&manifest_dir, "blast", BLAST_VERSION)
            .map_err(blackflower_build_error)?;
    let physx_root = workspace_root.join("vendor/PhysX");
    blackflower_build::validate_vendor_source_revision(&blast, &physx_root)
        .map_err(blackflower_build_error)?;
    let blast_library = blackflower_build::find_static_library(
        &blast,
        &configuration,
        "blackflower_blast",
        "blackflower_blast",
    )
    .map_err(blackflower_build_error)?;
    let has_stress = env::var("CARGO_CFG_TARGET_ARCH").is_ok_and(|arch| arch == "x86_64");

    let install_dir = compile_wrapper(&configuration, &physx_root, &blast_library, has_stress);
    generate_bindings()?;
    link_native(&install_dir, &blast_library)?;
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
    blast_library: &Path,
    has_stress: bool,
) -> PathBuf {
    let mut config = cmake::Config::new("native");
    config
        .profile(configuration.cmake_profile)
        .static_crt(configuration.crt_static)
        .define("BLACKFLOWER_PHYSX_ROOT", physx_root)
        .define("BLACKFLOWER_BLAST_LIBRARY", blast_library)
        .define(
            "BLACKFLOWER_BLAST_HAS_STRESS",
            if has_stress { "1" } else { "0" },
        );
    config.build()
}

fn generate_bindings() -> Result<(), Box<dyn Error>> {
    let out_dir = PathBuf::from(env::var_os("OUT_DIR").ok_or("OUT_DIR is not set")?);
    let builder = bindgen::Builder::default()
        .header(WRAPPER_HEADER)
        .allowlist_function("^bf_destruction_.*")
        .allowlist_type("^BFDestruction.*")
        .allowlist_var("^BF_DESTRUCTION_.*")
        .derive_default(true)
        .generate_comments(false)
        .layout_tests(false)
        .parse_callbacks(Box::new(bindgen::CargoCallbacks::new()));
    let generation = catch_unwind(AssertUnwindSafe(|| builder.generate())).map_err(|_payload| {
        "failed to load libclang for Blast bindings; install libclang and set LIBCLANG_PATH"
    })?;
    generation
        .map_err(|error| format!("failed to generate Blast bindings: {error}"))?
        .write_to_file(out_dir.join("blast_bindings.rs"))?;
    Ok(())
}

fn link_native(install_dir: &Path, blast_library: &Path) -> Result<(), Box<dyn Error>> {
    for directory in ["lib", "lib64"] {
        let path = install_dir.join(directory);
        if path.is_dir() {
            println!("cargo:rustc-link-search=native={}", path.display());
        }
    }
    println!("cargo:rustc-link-lib=static=blackflower_destruction_wrapper");
    blackflower_build::emit_static_library(blast_library).map_err(blackflower_build_error)?;

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
