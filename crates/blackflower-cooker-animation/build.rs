use std::env;
use std::error::Error;
use std::path::{Path, PathBuf};

const OZZ_VERSION: &str = "0.16.0";

fn main() -> Result<(), Box<dyn Error>> {
    blackflower_build::emit_rerun_environment();
    let manifest_dir =
        PathBuf::from(env::var_os("CARGO_MANIFEST_DIR").ok_or("CARGO_MANIFEST_DIR is not set")?);
    let (_configuration, _workspace_root, ozz) =
        blackflower_build::locate_from_cargo_build_script(&manifest_dir, "ozz", OZZ_VERSION)
            .map_err(blackflower_build_error)?;

    let executable = tool_path(&ozz);
    if !executable.is_file() {
        return Err(format!(
            "ozz build did not install gltf2ozz at `{}`",
            executable.display()
        )
        .into());
    }
    println!(
        "cargo:rustc-env=BLACKFLOWER_GLTF2OZZ={}",
        executable.display()
    );
    Ok(())
}

fn blackflower_build_error(error: Box<dyn Error + Send + Sync>) -> std::io::Error {
    std::io::Error::other(error.to_string())
}

fn tool_path(install: &Path) -> PathBuf {
    let executable = if env::var("CARGO_CFG_TARGET_OS").as_deref() == Ok("windows") {
        "gltf2ozz.exe"
    } else {
        "gltf2ozz"
    };
    install.join("bin/tools").join(executable)
}
