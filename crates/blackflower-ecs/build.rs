use std::env;
use std::error::Error;
use std::panic::{AssertUnwindSafe, catch_unwind};
use std::path::{Path, PathBuf};

const FLECS_VERSION: &str = "4.1.6";
const WRAPPER_HEADER: &str = "native/wrapper.h";
const CONFIG_HEADER: &str = "native/flecs_config.h";

fn main() -> Result<(), Box<dyn Error>> {
    for path in [WRAPPER_HEADER, CONFIG_HEADER] {
        println!("cargo:rerun-if-changed={path}");
        require_file(Path::new(path))?;
    }
    println!("cargo:rerun-if-env-changed=CARGO_FEATURE_METRICS");
    blackflower_build::emit_rerun_environment();

    let manifest_dir =
        PathBuf::from(env::var_os("CARGO_MANIFEST_DIR").ok_or("CARGO_MANIFEST_DIR is not set")?);
    let (configuration, workspace_root, flecs) =
        blackflower_build::locate_from_cargo_build_script(&manifest_dir, "flecs", FLECS_VERSION)
            .map_err(blackflower_build_error)?;
    let flecs_include = workspace_root.join("vendor/flecs/distr");
    let flecs_library =
        blackflower_build::find_static_library(&flecs, &configuration, "flecs", "flecs")
            .map_err(blackflower_build_error)?;

    generate_bindings(
        &flecs_include,
        env::var_os("CARGO_FEATURE_METRICS").is_some(),
    )?;
    blackflower_build::emit_static_library(&flecs_library).map_err(blackflower_build_error)?;
    link_platform_libraries()?;
    Ok(())
}

fn require_file(path: &Path) -> Result<(), Box<dyn Error>> {
    if path.is_file() {
        Ok(())
    } else {
        Err(format!("missing {}", path.display()).into())
    }
}

fn generate_bindings(flecs_include: &Path, stats_enabled: bool) -> Result<(), Box<dyn Error>> {
    let out_dir = PathBuf::from(env::var_os("OUT_DIR").ok_or("OUT_DIR is not set")?);
    let mut builder = bindgen::Builder::default()
        .header(WRAPPER_HEADER)
        .clang_arg("-DFLECS_CONFIG_HEADER")
        .clang_arg("-Inative")
        .clang_arg(format!("-I{}", flecs_include.display()))
        .allowlist_function("^(ecs_|Flecs).*")
        .allowlist_type("^(ecs_|Ecs).*")
        .allowlist_var("^(Ecs|FLECS_|ECS_).*")
        .derive_default(true)
        .generate_comments(false)
        .layout_tests(false)
        .parse_callbacks(Box::new(bindgen::CargoCallbacks::new()));
    if stats_enabled {
        builder = builder.clang_arg("-DBLACKFLOWER_ECS_ENABLE_STATS");
    }
    let generation = catch_unwind(AssertUnwindSafe(|| builder.generate())).map_err(|_payload| {
        "failed to load libclang for Flecs bindings; install libclang and set LIBCLANG_PATH"
    })?;
    generation
        .map_err(|error| format!("failed to generate Flecs bindings: {error}"))?
        .write_to_file(out_dir.join("flecs_bindings.rs"))?;
    Ok(())
}

fn link_platform_libraries() -> Result<(), Box<dyn Error>> {
    match env::var("CARGO_CFG_TARGET_OS")?.as_str() {
        "linux" => {
            println!("cargo:rustc-link-lib=m");
            println!("cargo:rustc-link-lib=pthread");
            println!("cargo:rustc-link-lib=rt");
        }
        "windows" => println!("cargo:rustc-link-lib=dbghelp"),
        _ => {}
    }
    Ok(())
}

fn blackflower_build_error(error: Box<dyn Error + Send + Sync>) -> std::io::Error {
    std::io::Error::other(error.to_string())
}
