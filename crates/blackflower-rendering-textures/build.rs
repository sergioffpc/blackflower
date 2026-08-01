use std::env;
use std::error::Error;
use std::panic::{AssertUnwindSafe, catch_unwind};
use std::path::{Path, PathBuf};

const KTX_VERSION: &str = "4.4.2";
const NATIVE_BUILD: &str = "native/CMakeLists.txt";
const WRAPPER_HEADER: &str = "native/wrapper.h";
const WRAPPER_SOURCE: &str = "native/wrapper.cpp";

fn main() -> Result<(), Box<dyn Error>> {
    let manifest_dir =
        PathBuf::from(env::var_os("CARGO_MANIFEST_DIR").ok_or("CARGO_MANIFEST_DIR is not set")?);
    for path in [NATIVE_BUILD, WRAPPER_HEADER, WRAPPER_SOURCE] {
        println!("cargo:rerun-if-changed={path}");
        require_file(Path::new(path))?;
    }
    blackflower_build::emit_cargo_directives();
    let (configuration, workspace_root, ktx) =
        blackflower_build::locate_from_cargo_build_script(&manifest_dir, "ktx", KTX_VERSION)
            .map_err(blackflower_build_error)?;
    let ktx_source = workspace_root.join("vendor/KTX-Software");
    let ktx_library = blackflower_build::find_static_library(&ktx, &configuration, "ktx", "ktx")
        .map_err(blackflower_build_error)?;
    let astc_library = (!configuration.target.contains("apple-darwin"))
        .then(|| {
            blackflower_build::find_static_library(
                &ktx,
                &configuration,
                "astcenc-none-static",
                "astcenc-none-static",
            )
            .map_err(blackflower_build_error)
        })
        .transpose()?;
    println!("cargo:rustc-env=BLACKFLOWER_KTX_VERSION={KTX_VERSION}");

    let install_dir = compile_wrapper(&configuration, &ktx_source, &ktx_library);
    generate_bindings()?;
    link_native(&install_dir, &ktx_library, astc_library.as_deref())?;
    Ok(())
}

fn require_file(path: &Path) -> Result<(), Box<dyn Error>> {
    if path.is_file() {
        return Ok(());
    }
    Err(format!(
        "missing {}; initialize KTX-Software with `git submodule update --init --recursive \
         vendor/KTX-Software`",
        path.display()
    )
    .into())
}

fn compile_wrapper(
    configuration: &blackflower_build::Configuration,
    ktx_source: &Path,
    ktx_library: &Path,
) -> PathBuf {
    let mut config = cmake::Config::new("native");
    config
        .profile(configuration.cmake_profile)
        .static_crt(configuration.crt_static)
        .define("BLACKFLOWER_KTX_VERSION", KTX_VERSION)
        .define("BLACKFLOWER_KTX_ROOT", ktx_source)
        .define("BLACKFLOWER_KTX_LIBRARY", ktx_library);
    config.build()
}

fn generate_bindings() -> Result<(), Box<dyn Error>> {
    let out_dir = PathBuf::from(env::var_os("OUT_DIR").ok_or("OUT_DIR is not set")?);
    let builder = bindgen::Builder::default()
        .header(WRAPPER_HEADER)
        .allowlist_function("^bf_texture_.*")
        .allowlist_type("^BFTexture.*")
        .allowlist_var("^BF_TEXTURE_.*")
        .derive_default(true)
        .generate_comments(false)
        .layout_tests(false)
        .parse_callbacks(Box::new(bindgen::CargoCallbacks::new()));
    let generation = catch_unwind(AssertUnwindSafe(|| builder.generate())).map_err(|_payload| {
        "failed to load libclang for KTX bindings; install libclang and set \
         LIBCLANG_PATH to the directory containing the shared library"
    })?;
    let bindings = generation.map_err(|error| {
        format!(
            "failed to generate KTX bindings; install libclang and set \
             LIBCLANG_PATH if it is not discoverable: {error}"
        )
    })?;
    bindings.write_to_file(out_dir.join("ktx_bindings.rs"))?;
    Ok(())
}

fn link_native(install_dir: &Path, ktx: &Path, astc: Option<&Path>) -> Result<(), Box<dyn Error>> {
    for directory in ["lib", "lib64"] {
        let path = install_dir.join(directory);
        if path.is_dir() {
            println!("cargo:rustc-link-search=native={}", path.display());
        }
    }
    println!("cargo:rustc-link-lib=static=blackflower_texture_wrapper");
    blackflower_build::emit_static_library(ktx).map_err(blackflower_build_error)?;

    if let Some(astc) = astc {
        blackflower_build::emit_static_library(astc).map_err(blackflower_build_error)?;
    }
    let target_os = env::var("CARGO_CFG_TARGET_OS")?;
    let target_env = env::var("CARGO_CFG_TARGET_ENV")?;
    match target_os.as_str() {
        "linux" | "android" => {
            println!("cargo:rustc-link-lib=stdc++");
            println!("cargo:rustc-link-lib=dl");
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
