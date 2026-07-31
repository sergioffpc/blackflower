use std::env;
use std::error::Error;
use std::ffi::OsStr;
use std::fs;
use std::path::{Path, PathBuf};

const NATIVE_BUILD: &str = "native/CMakeLists.txt";
const NATIVE_SOURCE: &str = "native/volume_cooker.cpp";
const OPENVDB_ROOT: &str = "../blackflower-rendering-volumes/vendor/openvdb";
const BOOST_ROOT: &str = "vendor/boost";
const TBB_ROOT: &str = "vendor/oneTBB";
const BLOSC_ROOT: &str = "vendor/c-blosc";
const ZLIB_ROOT: &str = "../../vendor/zlib";

fn main() -> Result<(), Box<dyn Error>> {
    for path in [
        NATIVE_BUILD,
        NATIVE_SOURCE,
        &format!("{OPENVDB_ROOT}/CMakeLists.txt"),
        &format!("{BOOST_ROOT}/CMakeLists.txt"),
        &format!("{BOOST_ROOT}/libs/interprocess/include/boost/interprocess/file_mapping.hpp"),
        &format!("{BOOST_ROOT}/libs/iostreams/include/boost/iostreams/copy.hpp"),
        &format!("{TBB_ROOT}/CMakeLists.txt"),
        &format!("{BLOSC_ROOT}/CMakeLists.txt"),
        &format!("{ZLIB_ROOT}/CMakeLists.txt"),
    ] {
        require_file(path)?;
        println!("cargo:rerun-if-changed={path}");
    }
    println!("cargo:rerun-if-changed=native");

    let out_dir = PathBuf::from(env::var_os("OUT_DIR").ok_or("OUT_DIR is not set")?);
    let zlib = build_zlib(&out_dir)?;
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

fn build_zlib(out_dir: &Path) -> Result<PathBuf, Box<dyn Error>> {
    let source = stage_source(Path::new(ZLIB_ROOT), out_dir, "zlib")?;
    let mut config = base_config(&source, &out_dir.join("zlib"));
    config.define("ZLIB_BUILD_EXAMPLES", "OFF");
    Ok(config.build())
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

fn stage_source(source: &Path, out_dir: &Path, name: &str) -> Result<PathBuf, Box<dyn Error>> {
    let destination = out_dir.join("native-sources").join(name);
    if destination.exists() {
        fs::remove_dir_all(&destination)?;
    }
    copy_tree(source, &destination)?;
    Ok(destination)
}

fn copy_tree(source: &Path, destination: &Path) -> Result<(), Box<dyn Error>> {
    fs::create_dir_all(destination)?;
    for entry in fs::read_dir(source)? {
        let entry = entry?;
        if entry.file_name() == OsStr::new(".git") {
            continue;
        }
        let source_path = entry.path();
        let destination_path = destination.join(entry.file_name());
        if entry.file_type()?.is_dir() {
            copy_tree(&source_path, &destination_path)?;
        } else if fs::hard_link(&source_path, &destination_path).is_err() {
            fs::copy(source_path, destination_path)?;
        }
    }
    Ok(())
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
