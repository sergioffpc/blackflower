#[allow(
    dead_code,
    reason = "the shared module exposes both producer and consumer halves of the native contract"
)]
#[path = "../../build-support/native_vendors.rs"]
mod native_vendors;

use std::env;
use std::error::Error;
use std::ffi::OsStr;
use std::path::{Path, PathBuf};

const NATIVE_BUILD: &str = "native/CMakeLists.txt";
const NATIVE_SOURCE: &str = "native/volume_cooker.cpp";
const OPENVDB_ROOT: &str = "../blackflower-rendering-volumes/vendor/openvdb";
const BOOST_ROOT: &str = "vendor/boost";
const TBB_ROOT: &str = "vendor/oneTBB";
const BLOSC_ROOT: &str = "vendor/c-blosc";
const ZLIB_VERSION: &str = "1.3.1";

fn main() -> Result<(), Box<dyn Error>> {
    println!("cargo:rerun-if-changed=../../build-support/native_vendors.rs");
    for path in [
        NATIVE_BUILD,
        NATIVE_SOURCE,
        &format!("{OPENVDB_ROOT}/CMakeLists.txt"),
        &format!("{BOOST_ROOT}/CMakeLists.txt"),
        &format!("{BOOST_ROOT}/libs/interprocess/include/boost/interprocess/file_mapping.hpp"),
        &format!("{BOOST_ROOT}/libs/iostreams/include/boost/iostreams/copy.hpp"),
        &format!("{TBB_ROOT}/CMakeLists.txt"),
        &format!("{BLOSC_ROOT}/CMakeLists.txt"),
    ] {
        require_file(path)?;
        println!("cargo:rerun-if-changed={path}");
    }
    println!("cargo:rerun-if-changed=native");
    native_vendors::emit_rerun_environment();

    let out_dir = PathBuf::from(env::var_os("OUT_DIR").ok_or("OUT_DIR is not set")?);
    let zlib = locate_shared_zlib()?;
    let blosc = build_blosc(&out_dir);
    let tbb = build_tbb(&out_dir);
    let native = build_cooker(&out_dir, &zlib, &blosc, &tbb);
    let executable = tool_path(&native);
    if !executable.is_file() {
        return Err(format!(
            "volume cooker build did not install `{}`",
            executable.display()
        )
        .into());
    }
    println!(
        "cargo:rustc-env=BLACKFLOWER_VDB_COOKER={}",
        executable.display()
    );
    Ok(())
}

fn require_file(path: &str) -> Result<(), Box<dyn Error>> {
    if Path::new(path).is_file() {
        Ok(())
    } else {
        Err(format!(
            "missing {path}; initialize the volume cooker submodules with \
             `.github/scripts/init-ci-submodules.sh assets`"
        )
        .into())
    }
}

fn locate_shared_zlib() -> Result<PathBuf, Box<dyn Error>> {
    let configuration =
        native_vendors::Configuration::from_cargo_build_script().map_err(native_contract_error)?;
    let manifest_dir =
        PathBuf::from(env::var_os("CARGO_MANIFEST_DIR").ok_or("CARGO_MANIFEST_DIR is not set")?);
    let workspace_root =
        native_vendors::find_workspace_root(&manifest_dir).map_err(native_contract_error)?;
    native_vendors::locate_vendor(&workspace_root, &configuration, "zlib", ZLIB_VERSION)
        .map_err(|error| native_contract_error(error).into())
}

fn native_contract_error(error: Box<dyn Error + Send + Sync>) -> std::io::Error {
    std::io::Error::other(error.to_string())
}

fn build_blosc(out_dir: &Path) -> PathBuf {
    let mut config = base_config(BLOSC_ROOT, &out_dir.join("blosc"));
    config
        .define("BUILD_SHARED", "OFF")
        .define("BUILD_STATIC", "ON")
        .define("BUILD_TESTS", "OFF")
        .define("BUILD_FUZZERS", "OFF")
        .define("BUILD_BENCHMARKS", "OFF")
        .define("CMAKE_POLICY_VERSION_MINIMUM", "3.5")
        .define("DEACTIVATE_SNAPPY", "ON")
        .define("DEACTIVATE_ZLIB", "ON")
        .define("DEACTIVATE_ZSTD", "ON")
        .define("BLOSC_INSTALL", "ON");
    config.build()
}

fn build_tbb(out_dir: &Path) -> PathBuf {
    let mut config = base_config(TBB_ROOT, &out_dir.join("tbb"));
    config
        .define("BUILD_SHARED_LIBS", "OFF")
        .define("TBB_TEST", "OFF")
        .define("TBBMALLOC_BUILD", "OFF")
        .define("TBBMALLOC_PROXY_BUILD", "OFF")
        .define("TBB_EXAMPLES", "OFF")
        .define("TBB_STRICT", "OFF");
    config.build()
}

fn build_cooker(out_dir: &Path, zlib: &Path, blosc: &Path, tbb: &Path) -> PathBuf {
    let mut config = base_config("native", &out_dir.join("native"));
    config
        .define("BLACKFLOWER_OPENVDB_ROOT", absolute(OPENVDB_ROOT))
        .define("BLACKFLOWER_BOOST_ROOT", absolute(BOOST_ROOT))
        .define("BLACKFLOWER_TBB_ROOT", tbb)
        .define("BLACKFLOWER_BLOSC_ROOT", blosc)
        .define("BLACKFLOWER_ZLIB_ROOT", zlib);
    config.build()
}

fn base_config(source: impl AsRef<Path>, output: &Path) -> cmake::Config {
    let mut config = cmake::Config::new(source);
    config.out_dir(output).profile("Release");
    if env::var_os("CARGO_CFG_TARGET_ENV").as_deref() == Some(OsStr::new("msvc")) {
        config
            .cxxflag("/EHsc")
            .define("CMAKE_MSVC_RUNTIME_LIBRARY", msvc_runtime());
    }
    config
}

fn msvc_runtime() -> &'static str {
    let target_features = env::var("CARGO_CFG_TARGET_FEATURE").unwrap_or_default();
    if target_features
        .split(',')
        .any(|feature| feature == "crt-static")
    {
        "MultiThreaded"
    } else {
        "MultiThreadedDLL"
    }
}

fn absolute(path: &str) -> PathBuf {
    PathBuf::from(env::var_os("CARGO_MANIFEST_DIR").unwrap_or_default()).join(path)
}

fn tool_path(install: &Path) -> PathBuf {
    let executable = if env::var("CARGO_CFG_TARGET_OS").as_deref() == Ok("windows") {
        "blackflower-vdb-cooker.exe"
    } else {
        "blackflower-vdb-cooker"
    };
    install.join("bin").join(executable)
}
