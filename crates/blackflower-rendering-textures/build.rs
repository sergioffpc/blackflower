use std::env;
use std::error::Error;
use std::panic::{AssertUnwindSafe, catch_unwind};
use std::path::{Path, PathBuf};
use std::process::Command;

const EXPECTED_KTX_COMMIT: &str = "4d6fc70eaf62ad0558e63e8d97eb9766118327a6";
const KTX_VERSION: &str = "4.4.2";
const KTX_ROOT: &str = "vendor/KTX-Software";
const KTX_BUILD: &str = "vendor/KTX-Software/CMakeLists.txt";
const KTX_HEADER: &str = "vendor/KTX-Software/include/ktx.h";
const NATIVE_BUILD: &str = "native/CMakeLists.txt";
const WRAPPER_HEADER: &str = "native/wrapper.h";
const WRAPPER_SOURCE: &str = "native/wrapper.cpp";

fn main() -> Result<(), Box<dyn Error>> {
    let manifest_dir =
        PathBuf::from(env::var_os("CARGO_MANIFEST_DIR").ok_or("CARGO_MANIFEST_DIR is not set")?);
    verify_ktx_submodule(&manifest_dir.join(KTX_ROOT))?;
    for path in [
        KTX_BUILD,
        KTX_HEADER,
        NATIVE_BUILD,
        WRAPPER_HEADER,
        WRAPPER_SOURCE,
    ] {
        println!("cargo:rerun-if-changed={path}");
        require_file(Path::new(path))?;
    }
    println!("cargo:rerun-if-changed=vendor/KTX-Software/include");
    println!("cargo:rerun-if-changed=vendor/KTX-Software/lib");
    println!("cargo:rerun-if-changed=vendor/KTX-Software/external/basisu");
    println!("cargo:rustc-env=BLACKFLOWER_KTX_VERSION={KTX_VERSION}");

    let install_dir = compile_native();
    generate_bindings()?;
    link_native(&install_dir)?;
    Ok(())
}

fn verify_ktx_submodule(ktx_root: &Path) -> Result<(), Box<dyn Error>> {
    let repository_root = PathBuf::from(git_output(ktx_root, &["rev-parse", "--show-toplevel"])?);
    if repository_root.canonicalize()? != ktx_root.canonicalize()? {
        return Err(format!("{} is not an initialized Git submodule", ktx_root.display()).into());
    }

    let commit = git_output(ktx_root, &["rev-parse", "HEAD"])?;
    if commit != EXPECTED_KTX_COMMIT {
        return Err(format!(
            "KTX-Software submodule commit is {commit}; expected {EXPECTED_KTX_COMMIT}"
        )
        .into());
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
            "git failed while verifying KTX-Software: {}",
            String::from_utf8_lossy(&output.stderr).trim()
        )
        .into());
    }
    Ok(String::from_utf8(output.stdout)?.trim().to_owned())
}

fn require_file(path: &Path) -> Result<(), Box<dyn Error>> {
    if path.is_file() {
        return Ok(());
    }
    Err(format!(
        "missing {}; initialize KTX-Software with `git submodule update --init \
         crates/blackflower-rendering-textures/vendor/KTX-Software`",
        path.display()
    )
    .into())
}

fn compile_native() -> PathBuf {
    let mut config = cmake::Config::new("native");
    config
        .define("BLACKFLOWER_KTX_VERSION", KTX_VERSION)
        .profile("Release")
        .build_target("blackflower_texture_install");
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

fn link_native(install_dir: &Path) -> Result<(), Box<dyn Error>> {
    let library_dir = install_dir.join("blackflower-ktx-lib");
    if !library_dir.is_dir() {
        return Err(format!(
            "KTX-Software build did not produce `{}`",
            library_dir.display()
        )
        .into());
    }
    println!("cargo:rustc-link-search=native={}", library_dir.display());
    println!("cargo:rustc-link-lib=static=blackflower_texture_wrapper");
    println!("cargo:rustc-link-lib=static=ktx");

    let target_os = env::var("CARGO_CFG_TARGET_OS")?;
    let target_env = env::var("CARGO_CFG_TARGET_ENV")?;
    if target_os != "macos" && target_os != "ios" {
        println!("cargo:rustc-link-lib=static=astcenc-none-static");
    }
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
