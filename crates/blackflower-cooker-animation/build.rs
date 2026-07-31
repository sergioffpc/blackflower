use std::env;
use std::error::Error;
use std::path::{Path, PathBuf};

const NATIVE_BUILD: &str = "native/CMakeLists.txt";
const OZZ_ROOT: &str = "../blackflower-animation/vendor/ozz-animation";

fn main() -> Result<(), Box<dyn Error>> {
    require_file(NATIVE_BUILD)?;
    require_file(&format!("{OZZ_ROOT}/CMakeLists.txt"))?;
    println!("cargo:rerun-if-changed={NATIVE_BUILD}");
    println!("cargo:rerun-if-changed={OZZ_ROOT}/CMakeLists.txt");
    println!("cargo:rerun-if-changed={OZZ_ROOT}/include");
    println!("cargo:rerun-if-changed={OZZ_ROOT}/src");

    let install = cmake::Config::new("native").profile("Release").build();
    let executable = tool_path(&install);
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

fn require_file(path: &str) -> Result<(), Box<dyn Error>> {
    if Path::new(path).is_file() {
        Ok(())
    } else {
        Err(format!(
            "missing {path}; initialize the ozz-animation submodule with \
             `git submodule update --init --recursive`"
        )
        .into())
    }
}

fn tool_path(install: &Path) -> PathBuf {
    let executable = if env::var("CARGO_CFG_TARGET_OS").as_deref() == Ok("windows") {
        "gltf2ozz.exe"
    } else {
        "gltf2ozz"
    };
    install.join("bin/tools").join(executable)
}
