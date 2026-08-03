use std::env;
use std::error::Error;
use std::panic::{AssertUnwindSafe, catch_unwind};
use std::path::{Path, PathBuf};

const NATIVE_BUILD: &str = "native/CMakeLists.txt";
const RECAST_VERSION: &str = "1.6.0";
const WRAPPER_HEADER: &str = "native/wrapper.h";
const WRAPPER_SOURCE: &str = "native/wrapper.cpp";

fn main() -> Result<(), Box<dyn Error>> {
    for path in [NATIVE_BUILD, WRAPPER_HEADER, WRAPPER_SOURCE] {
        require_file(path)?;
        println!("cargo:rerun-if-changed={path}");
    }
    blackflower_build::emit_cargo_directives();
    let manifest_dir =
        PathBuf::from(env::var_os("CARGO_MANIFEST_DIR").ok_or("CARGO_MANIFEST_DIR is not set")?);
    let (configuration, _workspace_root, recast) =
        blackflower_build::locate_from_cargo_build_script(&manifest_dir, "recast", RECAST_VERSION)
            .map_err(blackflower_build_error)?;
    let postfix = if configuration.cmake_profile == "Debug" {
        "-d"
    } else {
        ""
    };
    let recast_name = format!("Recast{postfix}");
    let detour_name = format!("Detour{postfix}");
    let recast_library =
        blackflower_build::find_static_library(&recast, &configuration, &recast_name, &recast_name)
            .map_err(blackflower_build_error)?;
    let detour_library =
        blackflower_build::find_static_library(&recast, &configuration, &detour_name, &detour_name)
            .map_err(blackflower_build_error)?;

    let mut config = cmake::Config::new("native");
    config
        .profile(configuration.cmake_profile)
        .static_crt(configuration.crt_static)
        .define("BLACKFLOWER_RECAST_INSTALL", &recast)
        .define("BLACKFLOWER_RECAST_LIBRARY", &recast_library)
        .define("BLACKFLOWER_DETOUR_LIBRARY", &detour_library);
    let install = config.build();
    generate_bindings()?;
    link_native(&install, &recast_library, &detour_library)?;
    Ok(())
}

fn require_file(path: &str) -> Result<(), Box<dyn Error>> {
    if Path::new(path).is_file() {
        Ok(())
    } else {
        Err(format!(
            "missing {path}; initialize RecastNavigation with \
             `git submodule update --init --recursive`"
        )
        .into())
    }
}

fn generate_bindings() -> Result<(), Box<dyn Error>> {
    let out_dir = PathBuf::from(env::var_os("OUT_DIR").ok_or("OUT_DIR is not set")?);
    let builder = bindgen::Builder::default()
        .header(WRAPPER_HEADER)
        .allowlist_function("^bf_navigation_cooker_.*")
        .allowlist_type("^BFNavigationCook.*")
        .allowlist_var("^BF_NAVIGATION_COOK_.*")
        .derive_default(true)
        .generate_comments(false)
        .layout_tests(false)
        .parse_callbacks(Box::new(bindgen::CargoCallbacks::new()));
    let generation = catch_unwind(AssertUnwindSafe(|| builder.generate()))
        .map_err(|_payload| "failed to load libclang for navigation cooker bindings")?;
    generation
        .map_err(|error| format!("failed to generate navigation cooker bindings: {error}"))?
        .write_to_file(out_dir.join("navigation_cooker_bindings.rs"))?;
    Ok(())
}

fn link_native(install: &Path, recast: &Path, detour: &Path) -> Result<(), Box<dyn Error>> {
    for directory in ["lib", "lib64"] {
        let path = install.join(directory);
        if path.is_dir() {
            println!("cargo:rustc-link-search=native={}", path.display());
        }
    }
    println!("cargo:rustc-link-lib=static=blackflower_navigation_cooker");
    blackflower_build::emit_static_library(recast).map_err(blackflower_build_error)?;
    blackflower_build::emit_static_library(detour).map_err(blackflower_build_error)?;
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
