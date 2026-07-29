use std::env;
use std::error::Error;
use std::fs;
use std::panic::{AssertUnwindSafe, catch_unwind};
use std::path::{Path, PathBuf};

const DETOUR_HEADER: &str = "vendor/recastnavigation/Detour/Include/DetourNavMesh.h";
const NATIVE_BUILD: &str = "native/CMakeLists.txt";
const RECAST_BUILD: &str = "vendor/recastnavigation/CMakeLists.txt";
const WRAPPER_HEADER: &str = "native/wrapper.h";
const WRAPPER_SOURCE: &str = "native/wrapper.cpp";

fn main() -> Result<(), Box<dyn Error>> {
    for path in [
        DETOUR_HEADER,
        NATIVE_BUILD,
        RECAST_BUILD,
        WRAPPER_HEADER,
        WRAPPER_SOURCE,
    ] {
        println!("cargo:rerun-if-changed={path}");
        require_file(path)?;
    }
    println!("cargo:rerun-if-changed=vendor/recastnavigation/Detour/Include");
    println!("cargo:rerun-if-changed=vendor/recastnavigation/Detour/Source");

    let version = read_recast_version()?;
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
        "missing {path}; initialize the RecastNavigation submodule with \
         `git submodule update --init --recursive`"
    )
    .into())
}

fn read_recast_version() -> Result<[u32; 3], Box<dyn Error>> {
    let source = fs::read_to_string(RECAST_BUILD)?;
    let version = source
        .lines()
        .find_map(|line| {
            line.trim()
                .strip_prefix("set(LIB_VERSION ")
                .and_then(|value| value.strip_suffix(')'))
        })
        .ok_or("RecastNavigation does not declare LIB_VERSION")?;
    let mut components = version.split('.');
    let parsed = [
        components.next().ok_or("missing major version")?.parse()?,
        components.next().ok_or("missing minor version")?.parse()?,
        components.next().ok_or("missing patch version")?.parse()?,
    ];
    if components.next().is_some() {
        return Err("RecastNavigation LIB_VERSION has too many components".into());
    }
    Ok(parsed)
}

fn compile_native(version: [u32; 3]) -> Result<PathBuf, Box<dyn Error>> {
    let mut config = cmake::Config::new("native");
    config
        .profile("Release")
        .define("BLACKFLOWER_RECAST_VERSION_MAJOR", version[0].to_string())
        .define("BLACKFLOWER_RECAST_VERSION_MINOR", version[1].to_string())
        .define("BLACKFLOWER_RECAST_VERSION_PATCH", version[2].to_string());
    Ok(config.build())
}

fn generate_bindings() -> Result<(), Box<dyn Error>> {
    let out_dir = PathBuf::from(env::var_os("OUT_DIR").ok_or("OUT_DIR is not set")?);
    let builder = bindgen::Builder::default()
        .header(WRAPPER_HEADER)
        .allowlist_function("^bf_navigation_.*")
        .allowlist_type("^BFNavigation.*")
        .allowlist_var("^BF_NAVIGATION_.*")
        .derive_default(true)
        .generate_comments(false)
        .layout_tests(false)
        .parse_callbacks(Box::new(bindgen::CargoCallbacks::new()));
    let generation = catch_unwind(AssertUnwindSafe(|| builder.generate())).map_err(|_payload| {
        "failed to load libclang for RecastNavigation bindings; install libclang and set \
         LIBCLANG_PATH to the directory containing the shared library"
    })?;
    let bindings = generation.map_err(|error| {
        format!(
            "failed to generate RecastNavigation bindings; install libclang and set \
             LIBCLANG_PATH if it is not discoverable: {error}"
        )
    })?;

    bindings.write_to_file(out_dir.join("recastnavigation_bindings.rs"))?;
    Ok(())
}

fn link_native(install_dir: &Path) -> Result<(), Box<dyn Error>> {
    for directory in ["lib", "lib64"] {
        let path = install_dir.join(directory);
        if path.is_dir() {
            println!("cargo:rustc-link-search=native={}", path.display());
        }
    }
    println!("cargo:rustc-link-lib=static=blackflower_navigation_wrapper");
    println!("cargo:rustc-link-lib=static=Detour");

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
