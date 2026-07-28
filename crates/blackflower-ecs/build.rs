use std::env;
use std::error::Error;
use std::panic::{AssertUnwindSafe, catch_unwind};
use std::path::{Path, PathBuf};

const FLECS_SOURCE: &str = "vendor/flecs/distr/flecs.c";
const FLECS_HEADER: &str = "vendor/flecs/distr/flecs.h";
const WRAPPER_HEADER: &str = "native/wrapper.h";
const CONFIG_HEADER: &str = "native/flecs_config.h";

fn main() -> Result<(), Box<dyn Error>> {
    for path in [FLECS_SOURCE, FLECS_HEADER, WRAPPER_HEADER, CONFIG_HEADER] {
        println!("cargo:rerun-if-changed={path}");
        require_file(path)?;
    }
    println!("cargo:rerun-if-env-changed=CARGO_FEATURE_METRICS");

    let stats_enabled = env::var_os("CARGO_FEATURE_METRICS").is_some();
    compile_flecs(stats_enabled);
    generate_bindings(stats_enabled)?;
    link_platform_libraries()?;

    Ok(())
}

fn require_file(path: &str) -> Result<(), Box<dyn Error>> {
    if Path::new(path).is_file() {
        return Ok(());
    }

    Err(format!(
        "missing {path}; initialize the Flecs submodule with \
         `git submodule update --init --recursive`"
    )
    .into())
}

fn compile_flecs(stats_enabled: bool) {
    let mut build = cc::Build::new();
    build
        .file(FLECS_SOURCE)
        .include("vendor/flecs/distr")
        .include("native")
        .define("FLECS_CONFIG_HEADER", None)
        .warnings(false);
    if stats_enabled {
        build.define("BLACKFLOWER_ECS_ENABLE_STATS", None);
    }
    build.compile("flecs");
}

fn generate_bindings(stats_enabled: bool) -> Result<(), Box<dyn Error>> {
    let out_dir = PathBuf::from(env::var_os("OUT_DIR").ok_or("OUT_DIR is not set")?);
    let mut builder = bindgen::Builder::default()
        .header(WRAPPER_HEADER)
        .clang_arg("-DFLECS_CONFIG_HEADER")
        .clang_arg("-Inative")
        .clang_arg("-Ivendor/flecs/distr")
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
        "failed to load libclang for Flecs bindings; install libclang and set \
         LIBCLANG_PATH to the directory containing the shared library"
    })?;
    let bindings = generation.map_err(|error| {
        format!(
            "failed to generate Flecs bindings; install libclang and set \
                 LIBCLANG_PATH if it is not discoverable: {error}"
        )
    })?;

    bindings.write_to_file(out_dir.join("flecs_bindings.rs"))?;
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
