use std::env;
use std::error::Error;
use std::panic::{AssertUnwindSafe, catch_unwind};
use std::path::{Path, PathBuf};

const JOLT_HEADER: &str = "vendor/JoltPhysics/Jolt/Jolt.h";
const NATIVE_BUILD: &str = "native/CMakeLists.txt";
const WRAPPER_HEADER: &str = "native/wrapper.h";
const WRAPPER_SOURCE: &str = "native/wrapper.cpp";
const X86_INSTRUCTION_SETS: [&str; 9] = [
    "USE_AVX",
    "USE_AVX2",
    "USE_AVX512",
    "USE_F16C",
    "USE_FMADD",
    "USE_LZCNT",
    "USE_SSE4_1",
    "USE_SSE4_2",
    "USE_TZCNT",
];

fn main() -> Result<(), Box<dyn Error>> {
    for path in [JOLT_HEADER, NATIVE_BUILD, WRAPPER_HEADER, WRAPPER_SOURCE] {
        println!("cargo:rerun-if-changed={path}");
        require_file(path)?;
    }
    println!("cargo:rerun-if-changed=vendor/JoltPhysics/Jolt");

    let install_dir = compile_native()?;
    generate_bindings()?;
    link_native(&install_dir)?;

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

fn compile_native() -> Result<PathBuf, Box<dyn Error>> {
    let target_os = env::var("CARGO_CFG_TARGET_OS")?;
    let target_arch = env::var("CARGO_CFG_TARGET_ARCH")?;
    let mut config = cmake::Config::new("native");
    config
        .profile("Distribution")
        .define("CROSS_PLATFORM_DETERMINISTIC", "ON")
        .define("DEBUG_RENDERER_IN_DEBUG_AND_RELEASE", "OFF")
        .define("DEBUG_RENDERER_IN_DISTRIBUTION", "OFF")
        .define("ENABLE_ALL_WARNINGS", "OFF")
        .define("ENABLE_INSTALL", "OFF")
        .define("ENABLE_OBJECT_STREAM", "OFF")
        .define("GENERATE_DEBUG_SYMBOLS", "OFF")
        .define("INTERPROCEDURAL_OPTIMIZATION", "OFF")
        .define("JPH_BUILD_SHARED_LIBS", "OFF")
        .define("JPH_USE_CPU_COMPUTE", "OFF")
        .define("JPH_USE_DX12", "OFF")
        .define("JPH_USE_MTL", "OFF")
        .define("JPH_USE_VK", "OFF")
        .define("PROFILER_IN_DEBUG_AND_RELEASE", "OFF")
        .define("PROFILER_IN_DISTRIBUTION", "OFF");
    configure_instruction_sets(&mut config, &target_os, &target_arch);
    Ok(config.build())
}

fn configure_instruction_sets(config: &mut cmake::Config, target_os: &str, target_arch: &str) {
    let use_avx2 = target_os == "linux" && target_arch == "x86_64";
    for instruction_set in X86_INSTRUCTION_SETS {
        let enabled = use_avx2 && instruction_set == "USE_AVX2";
        config.define(instruction_set, if enabled { "ON" } else { "OFF" });
    }
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

fn link_native(install_dir: &Path) -> Result<(), Box<dyn Error>> {
    for directory in ["lib", "lib64"] {
        let path = install_dir.join(directory);
        if path.is_dir() {
            println!("cargo:rustc-link-search=native={}", path.display());
        }
    }
    println!("cargo:rustc-link-lib=static=blackflower_physics_wrapper");
    println!("cargo:rustc-link-lib=static=Jolt");

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
