use std::env;
use std::error::Error;
use std::ffi::OsStr;
use std::fs;
use std::panic::{AssertUnwindSafe, catch_unwind};
use std::path::{Path, PathBuf};
use std::process::Command;

const EXPECTED_OPUS_VERSION: &str = "1.5.2";
const EXPECTED_OPUS_COMMIT: &str = "ddbe48383984d56acd9e1ab6a090c54ca6b735a6";
const OPUS_ROOT: &str = "vendor/opus";
const OPUS_INCLUDE: &str = "vendor/opus/include";
const WRAPPER_HEADER: &str = "native/wrapper.h";

fn main() -> Result<(), Box<dyn Error>> {
    emit_rebuild_inputs()?;
    verify_opus_revision()?;

    let out_dir = PathBuf::from(env::var_os("OUT_DIR").ok_or("OUT_DIR is not set")?);
    generate_bindings(&out_dir)?;
    let opus = build_opus(&out_dir)?;
    emit_static_linking(&opus)?;
    Ok(())
}

fn emit_rebuild_inputs() -> Result<(), Box<dyn Error>> {
    for path in [OPUS_ROOT, WRAPPER_HEADER] {
        println!("cargo:rerun-if-changed={path}");
        require_path(Path::new(path))?;
    }
    Ok(())
}

fn require_path(path: &Path) -> Result<(), Box<dyn Error>> {
    if path.exists() {
        return Ok(());
    }

    Err(format!(
        "missing {}; initialize native submodules with \
         `git submodule update --init --recursive`",
        path.display()
    )
    .into())
}

fn verify_opus_revision() -> Result<(), Box<dyn Error>> {
    let manifest_dir =
        PathBuf::from(env::var_os("CARGO_MANIFEST_DIR").ok_or("CARGO_MANIFEST_DIR is not set")?);
    let opus_root = manifest_dir.join(OPUS_ROOT);
    let repository_root = PathBuf::from(git_output(&opus_root, &["rev-parse", "--show-toplevel"])?);
    if repository_root.canonicalize()? != opus_root.canonicalize()? {
        return Err(format!(
            "{} is not an initialized Git submodule",
            opus_root.display()
        )
        .into());
    }

    let commit = git_output(&opus_root, &["rev-parse", "HEAD"])?;
    if commit != EXPECTED_OPUS_COMMIT {
        return Err(
            format!("Opus submodule commit is {commit}; expected {EXPECTED_OPUS_COMMIT}").into(),
        );
    }

    Ok(())
}

fn git_output(repository: &Path, arguments: &[&str]) -> Result<String, Box<dyn Error>> {
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
            "git failed while verifying Opus: {}",
            String::from_utf8_lossy(&output.stderr).trim()
        )
        .into());
    }
    Ok(String::from_utf8(output.stdout)?.trim().to_owned())
}

fn generate_bindings(out_dir: &Path) -> Result<(), Box<dyn Error>> {
    let target = env::var("TARGET")?;
    let builder = bindgen::Builder::default()
        .header(WRAPPER_HEADER)
        .clang_arg(format!("-I{OPUS_INCLUDE}"))
        .clang_arg(format!("--target={target}"))
        .allowlist_function("^opus_encoder_(create|destroy|ctl)$")
        .allowlist_function("^opus_decoder_(create|destroy|ctl)$")
        .allowlist_function("^opus_(encode|decode)_float$")
        .allowlist_function("^opus_(get_version_string|strerror)$")
        .allowlist_type("^Opus(Encoder|Decoder)$")
        .allowlist_var("^OPUS_.*")
        .derive_default(true)
        .generate_comments(false)
        .layout_tests(false)
        .prepend_enum_name(false)
        .wrap_unsafe_ops(true)
        .parse_callbacks(Box::new(bindgen::CargoCallbacks::new()));
    let generation = catch_unwind(AssertUnwindSafe(|| builder.generate())).map_err(|_payload| {
        "failed to load libclang for Opus bindings; install libclang and set \
         LIBCLANG_PATH to the directory containing the shared library"
    })?;
    let bindings = generation.map_err(|error| {
        format!(
            "failed to generate Opus bindings; install libclang and set \
             LIBCLANG_PATH if it is not discoverable: {error}"
        )
    })?;
    bindings.write_to_file(out_dir.join("opus_bindings.rs"))?;
    Ok(())
}

fn build_opus(out_dir: &Path) -> Result<PathBuf, Box<dyn Error>> {
    let output = out_dir.join("native/opus");
    let mut config = cmake::Config::new(OPUS_ROOT);
    config
        .out_dir(&output)
        .profile(native_profile())
        .build_target("opus")
        .define("BUILD_SHARED_LIBS", "OFF")
        .define("CMAKE_DISABLE_FIND_PACKAGE_Git", "ON")
        .define("CMAKE_POSITION_INDEPENDENT_CODE", "ON")
        .define("OPUS_PACKAGE_VERSION", EXPECTED_OPUS_VERSION)
        .define("OPUS_BUILD_SHARED_LIBRARY", "OFF")
        .define("OPUS_BUILD_TESTING", "OFF")
        .define("OPUS_BUILD_PROGRAMS", "OFF")
        .define("OPUS_CUSTOM_MODES", "OFF")
        .define("OPUS_FIXED_POINT", "OFF")
        .define("OPUS_ENABLE_FLOAT_API", "ON")
        .define("OPUS_HARDENING", "ON")
        .define("OPUS_NONTHREADSAFE_PSEUDOSTACK", "OFF")
        .define("OPUS_DRED", "OFF")
        .define("OPUS_OSCE", "OFF")
        .define("OPUS_INSTALL_PKG_CONFIG_MODULE", "OFF")
        .define("OPUS_INSTALL_CMAKE_CONFIG_MODULE", "OFF");
    if env::var_os("CARGO_CFG_TARGET_ENV").as_deref() == Some(OsStr::new("msvc")) {
        config.define("OPUS_STATIC_RUNTIME", static_crt_setting());
    }

    let destination = config.build();
    find_static_library(&destination)
}

fn native_profile() -> &'static str {
    if env::var_os("CARGO_CFG_TARGET_ENV").as_deref() == Some(OsStr::new("msvc")) {
        "RelWithDebInfo"
    } else if env::var_os("DEBUG").as_deref() == Some(OsStr::new("true")) {
        "Debug"
    } else {
        "Release"
    }
}

fn static_crt_setting() -> &'static str {
    let target_features = env::var("CARGO_CFG_TARGET_FEATURE").unwrap_or_default();
    if target_features
        .split(',')
        .any(|feature| feature == "crt-static")
    {
        "ON"
    } else {
        "OFF"
    }
}

fn find_static_library(root: &Path) -> Result<PathBuf, Box<dyn Error>> {
    let file_name = if target_is_windows() {
        "opus.lib"
    } else {
        "libopus.a"
    };
    find_built_file(root, file_name)
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

fn emit_static_linking(opus: &Path) -> Result<(), Box<dyn Error>> {
    let directory = opus
        .parent()
        .ok_or_else(|| format!("{} has no parent directory", opus.display()))?;
    println!("cargo:rustc-link-search=native={}", directory.display());
    println!("cargo:rustc-link-lib=static=opus");
    if env::var_os("CARGO_CFG_TARGET_OS").as_deref() == Some(OsStr::new("linux")) {
        println!("cargo:rustc-link-lib=dylib=m");
    }
    Ok(())
}
