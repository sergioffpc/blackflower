use std::env;
use std::error::Error;
use std::ffi::OsStr;
use std::fs;
use std::panic::{AssertUnwindSafe, catch_unwind};
use std::path::{Path, PathBuf};

const EMBREE_VERSION: &str = "4.4.1";
const WRAPPER_HEADER: &str = "native/wrapper.h";
const WRAPPER_SOURCE: &str = "native/wrapper.cpp";

struct EmbreeLibraries {
    include: PathBuf,
    lexers: PathBuf,
    math: PathBuf,
    simd: PathBuf,
    sys: PathBuf,
    tasking: PathBuf,
    sse2: PathBuf,
    sse4: Option<PathBuf>,
    avx: Option<PathBuf>,
    avx2: Option<PathBuf>,
}

fn main() -> Result<(), Box<dyn Error>> {
    for path in [WRAPPER_HEADER, WRAPPER_SOURCE] {
        println!("cargo:rerun-if-changed={path}");
        require_path(Path::new(path))?;
    }
    blackflower_build::emit_cargo_directives();
    require_supported_target()?;

    let configuration = blackflower_build::Configuration::from_cargo_build_script()
        .map_err(blackflower_build_error)?;
    let manifest_dir =
        PathBuf::from(env::var_os("CARGO_MANIFEST_DIR").ok_or("CARGO_MANIFEST_DIR is not set")?);
    let workspace_root =
        blackflower_build::find_workspace_root(&manifest_dir).map_err(blackflower_build_error)?;
    let embree =
        blackflower_build::locate_vendor(&workspace_root, &configuration, "embree", EMBREE_VERSION)
            .map_err(blackflower_build_error)?;
    let libraries = load_embree(&embree)?;
    let out_dir = PathBuf::from(env::var_os("OUT_DIR").ok_or("OUT_DIR is not set")?);
    compile_wrapper(&libraries)?;
    generate_bindings(&out_dir)?;
    emit_static_linking(&libraries)?;
    Ok(())
}

fn blackflower_build_error(error: Box<dyn Error + Send + Sync>) -> std::io::Error {
    std::io::Error::other(error.to_string())
}

fn require_supported_target() -> Result<(), Box<dyn Error>> {
    let architecture = env::var("CARGO_CFG_TARGET_ARCH")?;
    let operating_system = env::var("CARGO_CFG_TARGET_OS")?;
    if matches!(architecture.as_str(), "x86_64" | "aarch64")
        && matches!(operating_system.as_str(), "linux" | "macos" | "windows")
    {
        Ok(())
    } else {
        Err(format!(
            "blackflower-spatial-query does not support target {architecture}-{operating_system}"
        )
        .into())
    }
}

fn require_path(path: &Path) -> Result<(), Box<dyn Error>> {
    if path.exists() {
        Ok(())
    } else {
        Err(format!("missing {}", path.display()).into())
    }
}

fn load_embree(root: &Path) -> Result<EmbreeLibraries, Box<dyn Error>> {
    let architecture = env::var("CARGO_CFG_TARGET_ARCH")?;
    let operating_system = env::var("CARGO_CFG_TARGET_OS")?;
    let include = root.join("include/embree4");
    require_path(&include.join("rtcore.h"))?;
    let has_x86_isa_variants = architecture == "x86_64" && operating_system != "macos";
    let has_avx2_named_variant =
        has_x86_isa_variants || (architecture == "aarch64" && operating_system == "macos");
    Ok(EmbreeLibraries {
        include,
        lexers: find_static_library(root, "lexers", "lexers")?,
        math: find_static_library(root, "math", "math")?,
        simd: find_static_library(root, "simd", "simd")?,
        sys: find_static_library(root, "sys", "sys")?,
        tasking: find_static_library(root, "tasking", "tasking")?,
        sse2: find_static_library(root, "embree", "embree")?,
        sse4: has_x86_isa_variants
            .then(|| find_static_library(root, "embree_sse42", "embree_sse42"))
            .transpose()?,
        avx: has_x86_isa_variants
            .then(|| find_static_library(root, "embree_avx", "embree_avx"))
            .transpose()?,
        // Embree names its Apple NEON2X archive `embree_avx2` internally.
        avx2: has_avx2_named_variant
            .then(|| find_static_library(root, "embree_avx2", "embree_avx2"))
            .transpose()?,
    })
}

fn compile_wrapper(libraries: &EmbreeLibraries) -> Result<(), Box<dyn Error>> {
    let include_root = libraries
        .include
        .parent()
        .ok_or("Embree include directory has no parent")?;
    let mut build = cc::Build::new();
    build
        .cpp(true)
        .file(WRAPPER_SOURCE)
        .include(include_root)
        .warnings(false)
        .flag_if_supported("-std=c++17")
        .flag_if_supported("/std:c++17");
    build.compile("blackflower_spatial_query_wrapper");
    Ok(())
}

fn generate_bindings(out_dir: &Path) -> Result<(), Box<dyn Error>> {
    let builder = bindgen::Builder::default()
        .header(WRAPPER_HEADER)
        .allowlist_function("^bf_spatial_query_.*")
        .allowlist_type("^BFSpatialQuery.*")
        .allowlist_var("^BF_SPATIAL_QUERY_.*")
        .derive_default(true)
        .generate_comments(false)
        .layout_tests(false)
        .parse_callbacks(Box::new(bindgen::CargoCallbacks::new()));
    let generation = catch_unwind(AssertUnwindSafe(|| builder.generate())).map_err(|_payload| {
        "failed to load libclang for Embree bindings; install libclang and set LIBCLANG_PATH"
    })?;
    let bindings = generation.map_err(|error| {
        format!(
            "failed to generate Embree wrapper bindings; install libclang and set \
             LIBCLANG_PATH if it is not discoverable: {error}"
        )
    })?;
    bindings.write_to_file(out_dir.join("spatial_query_bindings.rs"))?;
    Ok(())
}

fn emit_static_linking(libraries: &EmbreeLibraries) -> Result<(), Box<dyn Error>> {
    let mut link_order = vec![&libraries.sse2];
    link_order.extend(libraries.sse4.iter());
    link_order.extend(libraries.avx.iter());
    link_order.extend(libraries.avx2.iter());
    link_order.extend([
        &libraries.tasking,
        &libraries.sys,
        &libraries.simd,
        &libraries.math,
        &libraries.lexers,
    ]);
    for library in link_order {
        let directory = library
            .parent()
            .ok_or_else(|| format!("{} has no parent directory", library.display()))?;
        println!("cargo:rustc-link-search=native={}", directory.display());
        println!("cargo:rustc-link-lib=static={}", static_link_name(library)?);
    }

    match env::var("CARGO_CFG_TARGET_OS")?.as_str() {
        "linux" => {
            println!("cargo:rustc-link-lib=dylib=stdc++");
            println!("cargo:rustc-link-lib=dylib=m");
            println!("cargo:rustc-link-lib=dylib=dl");
            println!("cargo:rustc-link-lib=dylib=pthread");
        }
        "macos" => println!("cargo:rustc-link-lib=dylib=c++"),
        "windows" => println!("cargo:rustc-link-lib=dylib=delayimp"),
        _ => {}
    }
    Ok(())
}

fn find_static_library(
    root: &Path,
    unix_name: &str,
    windows_name: &str,
) -> Result<PathBuf, Box<dyn Error>> {
    if target_is_windows() {
        let file_name = format!("{windows_name}.lib");
        find_built_file(root, &file_name).or_else(|_error| {
            let debug_file_name = format!("{windows_name}d.lib");
            find_built_file(root, &debug_file_name)
        })
    } else {
        find_built_file(root, &format!("lib{unix_name}.a"))
    }
}

fn target_is_windows() -> bool {
    env::var_os("CARGO_CFG_TARGET_OS").as_deref() == Some(OsStr::new("windows"))
}

fn find_built_file(root: &Path, file_name: &str) -> Result<PathBuf, Box<dyn Error>> {
    if !root.is_dir() {
        return Err(format!("native build directory {} does not exist", root.display()).into());
    }
    for entry in fs::read_dir(root)? {
        let entry = entry?;
        let path = entry.path();
        if entry.file_type()?.is_dir() {
            if let Ok(found) = find_built_file(&path, file_name) {
                return Ok(found);
            }
        } else if entry.file_name() == OsStr::new(file_name) {
            return Ok(path);
        }
    }
    Err(format!(
        "native build did not produce {file_name} below {}",
        root.display()
    )
    .into())
}

fn static_link_name(library: &Path) -> Result<&str, Box<dyn Error>> {
    let stem = library
        .file_stem()
        .and_then(OsStr::to_str)
        .ok_or_else(|| format!("invalid static library name {}", library.display()))?;
    Ok(stem.strip_prefix("lib").unwrap_or(stem))
}
