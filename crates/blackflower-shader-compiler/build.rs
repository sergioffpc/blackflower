use std::env;
use std::error::Error;
use std::panic::{AssertUnwindSafe, catch_unwind};
use std::path::{Path, PathBuf};
use std::process::Command;

const EXPECTED_SLANG_COMMIT: &str = "7c58a326b1f3812411a204b19cb01e323d8f6010";
const SLANG_ROOT: &str = "vendor/slang";
const SLANG_BUILD: &str = "vendor/slang/CMakeLists.txt";
const SLANG_HEADER: &str = "vendor/slang/include/slang.h";
const NATIVE_BUILD: &str = "native/CMakeLists.txt";
const WRAPPER_HEADER: &str = "native/wrapper.h";
const WRAPPER_SOURCE: &str = "native/wrapper.cpp";

struct RequiredSubmodule {
    path: &'static str,
    marker: &'static str,
}

const REQUIRED_SUBMODULES: &[RequiredSubmodule] = &[
    RequiredSubmodule {
        path: "external/cmark",
        marker: "CMakeLists.txt",
    },
    RequiredSubmodule {
        path: "external/fast_float",
        marker: "include/fast_float/fast_float.h",
    },
    RequiredSubmodule {
        path: "external/lz4",
        marker: "build/cmake/CMakeLists.txt",
    },
    RequiredSubmodule {
        path: "external/lua",
        marker: "onelua.c",
    },
    RequiredSubmodule {
        path: "external/miniz",
        marker: "CMakeLists.txt",
    },
    RequiredSubmodule {
        path: "external/spirv-headers",
        marker: "CMakeLists.txt",
    },
    RequiredSubmodule {
        path: "external/unordered_dense",
        marker: "CMakeLists.txt",
    },
    RequiredSubmodule {
        path: "external/vulkan",
        marker: "CMakeLists.txt",
    },
];

fn main() -> Result<(), Box<dyn Error>> {
    let manifest_dir =
        PathBuf::from(env::var_os("CARGO_MANIFEST_DIR").ok_or("CARGO_MANIFEST_DIR is not set")?);
    verify_slang_submodule(&manifest_dir.join(SLANG_ROOT))?;
    for path in [
        SLANG_BUILD,
        SLANG_HEADER,
        NATIVE_BUILD,
        WRAPPER_HEADER,
        WRAPPER_SOURCE,
    ] {
        println!("cargo:rerun-if-changed={path}");
        require_file(Path::new(path))?;
    }
    for dependency in REQUIRED_SUBMODULES {
        require_file(
            &Path::new(SLANG_ROOT)
                .join(dependency.path)
                .join(dependency.marker),
        )?;
    }
    println!("cargo:rerun-if-changed=vendor/slang/include");
    println!("cargo:rerun-if-changed=vendor/slang/source");

    let install_dir = compile_native();
    generate_bindings()?;
    link_native(&install_dir)?;
    Ok(())
}

fn verify_slang_submodule(slang_root: &Path) -> Result<(), Box<dyn Error>> {
    let repository_root = PathBuf::from(git_output(slang_root, &["rev-parse", "--show-toplevel"])?);
    if repository_root.canonicalize()? != slang_root.canonicalize()? {
        return Err(format!(
            "{} is not an initialized Git submodule",
            slang_root.display()
        )
        .into());
    }

    let commit = git_output(slang_root, &["rev-parse", "HEAD"])?;
    if commit != EXPECTED_SLANG_COMMIT {
        return Err(format!(
            "Slang submodule commit is {commit}; expected {EXPECTED_SLANG_COMMIT}"
        )
        .into());
    }

    let mut arguments = vec!["submodule", "status", "--"];
    arguments.extend(REQUIRED_SUBMODULES.iter().map(|dependency| dependency.path));
    let status = git_output_raw(slang_root, &arguments)?;
    let dependencies = status.lines().collect::<Vec<_>>();
    if dependencies.len() != REQUIRED_SUBMODULES.len()
        || dependencies.iter().any(|line| !line.starts_with(' '))
    {
        return Err(
            "Slang nested submodules are missing or not at their pinned commits; run \
             `git submodule update --init --recursive \
             crates/blackflower-shader-compiler/vendor/slang`"
                .into(),
        );
    }
    Ok(())
}

fn git_output(repository: &Path, arguments: &[&str]) -> Result<String, Box<dyn Error>> {
    Ok(git_output_raw(repository, arguments)?.trim().to_owned())
}

fn git_output_raw(repository: &Path, arguments: &[&str]) -> Result<String, Box<dyn Error>> {
    let output = Command::new("git")
        .env_remove("GIT_DIR")
        .env_remove("GIT_WORK_TREE")
        .env_remove("GIT_COMMON_DIR")
        .env_remove("GIT_INDEX_FILE")
        .arg("-C")
        .arg(repository)
        .args(arguments)
        .output()?;
    if !output.status.success() {
        return Err(format!(
            "git failed while verifying Slang: {}",
            String::from_utf8_lossy(&output.stderr).trim()
        )
        .into());
    }
    Ok(String::from_utf8(output.stdout)?.trim_end().to_owned())
}

fn require_file(path: &Path) -> Result<(), Box<dyn Error>> {
    if path.is_file() {
        return Ok(());
    }
    Err(format!(
        "missing {}; initialize the Slang submodule with \
         `git submodule update --init --recursive \
         crates/blackflower-shader-compiler/vendor/slang`",
        path.display()
    )
    .into())
}

fn compile_native() -> PathBuf {
    let mut config = cmake::Config::new("native");
    config
        .profile("Release")
        .build_target("blackflower_shader_compiler_install");
    config.build()
}

fn generate_bindings() -> Result<(), Box<dyn Error>> {
    let out_dir = PathBuf::from(env::var_os("OUT_DIR").ok_or("OUT_DIR is not set")?);
    let builder = bindgen::Builder::default()
        .header(WRAPPER_HEADER)
        .allowlist_function("^bf_shader_compiler_.*")
        .allowlist_type("^BFShaderCompiler.*")
        .allowlist_var("^BF_SHADER_.*")
        .derive_default(true)
        .generate_comments(false)
        .layout_tests(false)
        .parse_callbacks(Box::new(bindgen::CargoCallbacks::new()));
    let generation = catch_unwind(AssertUnwindSafe(|| builder.generate())).map_err(|_payload| {
        "failed to load libclang for Slang bindings; install libclang and set \
         LIBCLANG_PATH to the directory containing the shared library"
    })?;
    let bindings = generation.map_err(|error| {
        format!(
            "failed to generate Slang bindings; install libclang and set \
             LIBCLANG_PATH if it is not discoverable: {error}"
        )
    })?;
    bindings.write_to_file(out_dir.join("slang_bindings.rs"))?;
    Ok(())
}

fn link_native(install_dir: &Path) -> Result<(), Box<dyn Error>> {
    let library_dir = install_dir.join("blackflower-slang-lib");
    if !library_dir.is_dir() {
        return Err(format!("Slang build did not produce `{}`", library_dir.display()).into());
    }
    println!("cargo:rustc-link-search=native={}", library_dir.display());
    for library in [
        "blackflower_shader_compiler_wrapper",
        "blackflower_slang_compiler",
        "blackflower_slang_compiler_core",
        "blackflower_slang_core",
        "blackflower_slang_miniz",
        "blackflower_slang_lz4",
        "blackflower_slang_cmark_gfm",
    ] {
        println!("cargo:rustc-link-lib=static={library}");
    }

    let target_os = env::var("CARGO_CFG_TARGET_OS")?;
    let target_env = env::var("CARGO_CFG_TARGET_ENV")?;
    match target_os.as_str() {
        "linux" | "android" => {
            println!("cargo:rustc-link-lib=stdc++");
            println!("cargo:rustc-link-lib=dl");
        }
        "macos" | "ios" | "freebsd" => println!("cargo:rustc-link-lib=c++"),
        "windows" if target_env == "gnu" => println!("cargo:rustc-link-lib=stdc++"),
        _ => {}
    }
    Ok(())
}
