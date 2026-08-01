#[allow(
    dead_code,
    reason = "the shared module exposes both producer and consumer halves of the native contract"
)]
#[path = "../../tools/native/support/native_vendors.rs"]
mod native_vendors;

use std::env;
use std::error::Error;
use std::path::{Path, PathBuf};

const BLOSC_VERSION: &str = "1.21.6";
const OPENVDB_VERSION: &str = "13.0.0";
const TBB_VERSION: &str = "2022.1.0";
const ZLIB_VERSION: &str = "1.3.1";
const NATIVE_BUILD: &str = "native/CMakeLists.txt";
const NATIVE_SOURCE: &str = "native/wrapper.cpp";

struct CookerDependencies<'a> {
    openvdb: &'a Path,
    openvdb_library: &'a Path,
    blosc_library: &'a Path,
    tbb: &'a Path,
    tbb_library: &'a Path,
    zlib_library: &'a Path,
}

fn main() -> Result<(), Box<dyn Error>> {
    println!("cargo:rerun-if-changed=../../tools/native/support/native_vendors.rs");
    for path in [NATIVE_BUILD, NATIVE_SOURCE] {
        println!("cargo:rerun-if-changed={path}");
        require_file(Path::new(path))?;
    }
    native_vendors::emit_rerun_environment();

    let manifest_dir =
        PathBuf::from(env::var_os("CARGO_MANIFEST_DIR").ok_or("CARGO_MANIFEST_DIR is not set")?);
    let (configuration, workspace_root, openvdb) =
        native_vendors::locate_from_cargo_build_script(&manifest_dir, "openvdb", OPENVDB_VERSION)
            .map_err(native_contract_error)?;
    let blosc = locate(&manifest_dir, "blosc", BLOSC_VERSION)?;
    let tbb = locate(&manifest_dir, "tbb", TBB_VERSION)?;
    let zlib = locate(&manifest_dir, "zlib", ZLIB_VERSION)?;
    let openvdb_library =
        native_vendors::find_static_library(&openvdb, &configuration, "openvdb", "openvdb")
            .map_err(native_contract_error)?;
    let blosc_library =
        native_vendors::find_static_library(&blosc, &configuration, "blosc", "blosc")
            .map_err(native_contract_error)?;
    let (tbb_unix, tbb_windows) = if configuration.cmake_profile == "Debug" {
        ("tbb_debug", "tbb12_debug")
    } else {
        ("tbb", "tbb12")
    };
    let tbb_library =
        native_vendors::find_static_library(&tbb, &configuration, tbb_unix, tbb_windows)
            .map_err(native_contract_error)?;
    let zlib_library =
        native_vendors::find_static_library(&zlib, &configuration, "z", "zlibstatic")
            .map_err(native_contract_error)?;
    let executable = compile_cooker(
        &configuration,
        &workspace_root,
        &CookerDependencies {
            openvdb: &openvdb,
            openvdb_library: &openvdb_library,
            blosc_library: &blosc_library,
            tbb: &tbb,
            tbb_library: &tbb_library,
            zlib_library: &zlib_library,
        },
    );
    export_tool(&executable)
}

fn export_tool(executable: &Path) -> Result<(), Box<dyn Error>> {
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

fn compile_cooker(
    configuration: &native_vendors::Configuration,
    workspace_root: &Path,
    dependencies: &CookerDependencies<'_>,
) -> PathBuf {
    let mut config = cmake::Config::new("native");
    config
        .profile(configuration.cmake_profile)
        .static_crt(configuration.crt_static)
        .define("BLACKFLOWER_OPENVDB_INSTALL", dependencies.openvdb)
        .define(
            "BLACKFLOWER_OPENVDB_ROOT",
            workspace_root.join("vendor/openvdb"),
        )
        .define(
            "BLACKFLOWER_BOOST_ROOT",
            workspace_root.join("vendor/boost"),
        )
        .define("BLACKFLOWER_OPENVDB_LIBRARY", dependencies.openvdb_library)
        .define("BLACKFLOWER_BLOSC_LIBRARY", dependencies.blosc_library)
        .define("BLACKFLOWER_TBB_INSTALL", dependencies.tbb)
        .define("BLACKFLOWER_TBB_LIBRARY", dependencies.tbb_library)
        .define("BLACKFLOWER_ZLIB_LIBRARY", dependencies.zlib_library);
    let install = config.build();
    tool_path(&install)
}

fn locate(manifest_dir: &Path, name: &str, version: &str) -> Result<PathBuf, Box<dyn Error>> {
    native_vendors::locate_from_cargo_build_script(manifest_dir, name, version)
        .map(|(_configuration, _workspace_root, directory)| directory)
        .map_err(|error| native_contract_error(error).into())
}

fn require_file(path: &Path) -> Result<(), Box<dyn Error>> {
    if path.is_file() {
        Ok(())
    } else {
        Err(format!("missing {}", path.display()).into())
    }
}

fn tool_path(install: &Path) -> PathBuf {
    let executable = if env::var("CARGO_CFG_TARGET_OS").as_deref() == Ok("windows") {
        "blackflower-vdb-cooker.exe"
    } else {
        "blackflower-vdb-cooker"
    };
    install.join("bin").join(executable)
}

fn native_contract_error(error: Box<dyn Error + Send + Sync>) -> std::io::Error {
    std::io::Error::other(error.to_string())
}
