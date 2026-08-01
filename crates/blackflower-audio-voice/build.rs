#[allow(
    dead_code,
    reason = "the shared module exposes both producer and consumer halves of the native contract"
)]
#[path = "../../tools/native/support/native_vendors.rs"]
mod native_vendors;

use std::env;
use std::error::Error;
use std::panic::{AssertUnwindSafe, catch_unwind};
use std::path::{Path, PathBuf};

const OPUS_VERSION: &str = "1.5.2";
const WRAPPER_HEADER: &str = "native/wrapper.h";

fn main() -> Result<(), Box<dyn Error>> {
    println!("cargo:rerun-if-changed=../../tools/native/support/native_vendors.rs");
    println!("cargo:rerun-if-changed={WRAPPER_HEADER}");
    native_vendors::emit_rerun_environment();

    let manifest_dir =
        PathBuf::from(env::var_os("CARGO_MANIFEST_DIR").ok_or("CARGO_MANIFEST_DIR is not set")?);
    let (configuration, workspace_root, opus) =
        native_vendors::locate_from_cargo_build_script(&manifest_dir, "opus", OPUS_VERSION)
            .map_err(native_contract_error)?;
    let opus_include = workspace_root.join("vendor/opus/include");
    require_path(&opus_include.join("opus.h"))?;
    let opus_library = native_vendors::find_static_library(&opus, &configuration, "opus", "opus")
        .map_err(native_contract_error)?;

    generate_bindings(&opus_include)?;
    native_vendors::emit_static_library(&opus_library).map_err(native_contract_error)?;
    if env::var_os("CARGO_CFG_TARGET_OS").as_deref() == Some("linux".as_ref()) {
        println!("cargo:rustc-link-lib=dylib=m");
    }
    Ok(())
}

fn require_path(path: &Path) -> Result<(), Box<dyn Error>> {
    if path.exists() {
        Ok(())
    } else {
        Err(format!(
            "missing {}; initialize global native vendors",
            path.display()
        )
        .into())
    }
}

fn generate_bindings(opus_include: &Path) -> Result<(), Box<dyn Error>> {
    let out_dir = PathBuf::from(env::var_os("OUT_DIR").ok_or("OUT_DIR is not set")?);
    let target = env::var("TARGET")?;
    let builder = bindgen::Builder::default()
        .header(WRAPPER_HEADER)
        .clang_arg(format!("-I{}", opus_include.display()))
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
        "failed to load libclang for Opus bindings; install libclang and set LIBCLANG_PATH"
    })?;
    generation
        .map_err(|error| format!("failed to generate Opus bindings: {error}"))?
        .write_to_file(out_dir.join("opus_bindings.rs"))?;
    Ok(())
}

fn native_contract_error(error: Box<dyn Error + Send + Sync>) -> std::io::Error {
    std::io::Error::other(error.to_string())
}
