use std::env;
use std::error::Error;
use std::ffi::OsStr;
use std::panic::{AssertUnwindSafe, catch_unwind};
use std::path::{Path, PathBuf};

const RECAST_ROOT: &str = "../blackflower-navigation/vendor/recastnavigation";
const NATIVE_BUILD: &str = "native/CMakeLists.txt";
const WRAPPER_HEADER: &str = "native/cooker.h";
const WRAPPER_SOURCE: &str = "native/cooker.cpp";

fn main() -> Result<(), Box<dyn Error>> {
    for path in [
        NATIVE_BUILD,
        WRAPPER_HEADER,
        WRAPPER_SOURCE,
        &format!("{RECAST_ROOT}/Recast/Include/Recast.h"),
        &format!("{RECAST_ROOT}/Detour/Include/DetourNavMeshBuilder.h"),
    ] {
        require_file(path)?;
        println!("cargo:rerun-if-changed={path}");
    }
    println!("cargo:rerun-if-changed={RECAST_ROOT}/Recast/Include");
    println!("cargo:rerun-if-changed={RECAST_ROOT}/Recast/Source");
    println!("cargo:rerun-if-changed={RECAST_ROOT}/Detour/Include");
    println!("cargo:rerun-if-changed={RECAST_ROOT}/Detour/Source/DetourNavMeshBuilder.cpp");

    let mut config = cmake::Config::new("native");
    config
        .profile("Release")
        .define("BLACKFLOWER_RECAST_ROOT", absolute(RECAST_ROOT));
    if env::var_os("CARGO_CFG_TARGET_ENV").as_deref() == Some(OsStr::new("msvc")) {
        config
            .cxxflag("/EHsc")
            .define("CMAKE_MSVC_RUNTIME_LIBRARY", msvc_runtime());
    }
    let install = config.build();
    generate_bindings()?;
    link_native(&install)?;
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

fn absolute(path: &str) -> PathBuf {
    PathBuf::from(env::var_os("CARGO_MANIFEST_DIR").unwrap_or_default()).join(path)
}

fn msvc_runtime() -> &'static str {
    let features = env::var("CARGO_CFG_TARGET_FEATURE").unwrap_or_default();
    if features.split(',').any(|feature| feature == "crt-static") {
        "MultiThreaded"
    } else {
        "MultiThreadedDLL"
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

fn link_native(install: &Path) -> Result<(), Box<dyn Error>> {
    for directory in ["lib", "lib64"] {
        let path = install.join(directory);
        if path.is_dir() {
            println!("cargo:rustc-link-search=native={}", path.display());
        }
    }
    println!("cargo:rustc-link-lib=static=blackflower_navigation_cooker");
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
