use std::env;
use std::error::Error;
use std::ffi::OsStr;
use std::fs;
use std::panic::{AssertUnwindSafe, catch_unwind};
use std::path::{Path, PathBuf};

const EXPECTED_SDK_VERSION: &str = "4.8.1";
const SDK_ROOT: &str = "vendor/steam-audio-sdk/core";
const SDK_BUILD: &str = "vendor/steam-audio-sdk/core/CMakeLists.txt";
const SDK_INCLUDE: &str = "vendor/steam-audio-sdk/core/src/core";
const SDK_VERSION_TEMPLATE: &str = "vendor/steam-audio-sdk/core/src/core/phonon_version.h.in";
const FLATBUFFERS_ROOT: &str = "vendor/flatbuffers";
const MYSOFA_ROOT: &str = "vendor/libmysofa";
const PFFFT_ROOT: &str = "vendor/pffft";
const ZLIB_ROOT: &str = "vendor/zlib";
const WRAPPER_HEADER: &str = "native/wrapper.h";

struct NativeLibraries {
    flatbuffers_include: PathBuf,
    flatc: PathBuf,
    mysofa_include: PathBuf,
    mysofa_library: PathBuf,
    pffft_include: PathBuf,
    pffft_library: PathBuf,
    zlib_include: PathBuf,
    zlib_library: PathBuf,
}

fn main() -> Result<(), Box<dyn Error>> {
    emit_rebuild_inputs()?;
    let out_dir = PathBuf::from(env::var_os("OUT_DIR").ok_or("OUT_DIR is not set")?);
    let profile = native_profile();
    verify_sdk_version()?;
    generate_version_header(&out_dir)?;
    generate_bindings(&out_dir)?;

    let libraries = build_native_libraries(&out_dir, profile)?;
    let phonon = build_steam_audio(&out_dir, profile, &libraries)?;
    emit_static_linking(&phonon, &libraries)?;
    Ok(())
}

fn emit_rebuild_inputs() -> Result<(), Box<dyn Error>> {
    for path in [
        SDK_ROOT,
        FLATBUFFERS_ROOT,
        MYSOFA_ROOT,
        PFFFT_ROOT,
        ZLIB_ROOT,
        WRAPPER_HEADER,
    ] {
        println!("cargo:rerun-if-changed={path}");
        require_path(Path::new(path))?;
    }
    println!("cargo:rerun-if-env-changed=BLACKFLOWER_FLATC");
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

fn native_profile() -> &'static str {
    if env::var_os("CARGO_CFG_TARGET_ENV").as_deref() == Some(OsStr::new("msvc")) {
        "Release"
    } else if env::var_os("DEBUG").as_deref() == Some(OsStr::new("true")) {
        "Debug"
    } else {
        "Release"
    }
}

fn verify_sdk_version() -> Result<(), Box<dyn Error>> {
    let build = fs::read_to_string(SDK_BUILD)?;
    let version = build
        .lines()
        .find_map(|line| {
            line.trim()
                .strip_prefix("project(Phonon VERSION ")
                .and_then(|value| value.strip_suffix(')'))
        })
        .ok_or("Steam Audio does not declare its project version")?;
    if version == EXPECTED_SDK_VERSION {
        Ok(())
    } else {
        Err(
            format!("Steam Audio submodule version is {version}; expected {EXPECTED_SDK_VERSION}")
                .into(),
        )
    }
}

fn generate_version_header(out_dir: &Path) -> Result<(), Box<dyn Error>> {
    let mut components = EXPECTED_SDK_VERSION.split('.');
    let major = components
        .next()
        .ok_or("Steam Audio version has no major")?;
    let minor = components
        .next()
        .ok_or("Steam Audio version has no minor")?;
    let patch = components
        .next()
        .ok_or("Steam Audio version has no patch")?;
    if components.next().is_some() {
        return Err("Steam Audio version has more than three components".into());
    }

    let header = fs::read_to_string(SDK_VERSION_TEMPLATE)?
        .replace("@PROJECT_VERSION_MAJOR@", major)
        .replace("@PROJECT_VERSION_MINOR@", minor)
        .replace("@PROJECT_VERSION_PATCH@", patch);
    fs::write(out_dir.join("phonon_version.h"), header)?;
    Ok(())
}

fn generate_bindings(out_dir: &Path) -> Result<(), Box<dyn Error>> {
    let target = env::var("TARGET")?;
    let builder = bindgen::Builder::default()
        .header(WRAPPER_HEADER)
        .clang_arg(format!("-I{}", out_dir.display()))
        .clang_arg(format!("-I{SDK_INCLUDE}"))
        .clang_arg(format!("--target={target}"))
        .allowlist_function("^ipl[A-Z].*")
        .allowlist_type("^IPL.*")
        .allowlist_var("^(IPL_|STEAMAUDIO_).*")
        .derive_default(true)
        .generate_comments(false)
        .layout_tests(false)
        .prepend_enum_name(false)
        .wrap_unsafe_ops(true)
        .parse_callbacks(Box::new(bindgen::CargoCallbacks::new()));
    let generation = catch_unwind(AssertUnwindSafe(|| builder.generate())).map_err(|_payload| {
        "failed to load libclang for Steam Audio bindings; install libclang and set \
         LIBCLANG_PATH to the directory containing the shared library"
    })?;
    let bindings = generation.map_err(|error| {
        format!(
            "failed to generate Steam Audio bindings; install libclang and set \
             LIBCLANG_PATH if it is not discoverable: {error}"
        )
    })?;

    bindings.write_to_file(out_dir.join("steam_audio_bindings.rs"))?;
    Ok(())
}

fn build_native_libraries(
    out_dir: &Path,
    profile: &str,
) -> Result<NativeLibraries, Box<dyn Error>> {
    let (flatbuffers_include, flatc) = build_flatbuffers(out_dir, profile)?;
    let (zlib_include, zlib_library) = build_zlib(out_dir, profile)?;
    let (pffft_include, pffft_library) = build_pffft(out_dir, profile)?;
    let (mysofa_include, mysofa_library) =
        build_mysofa(out_dir, profile, &zlib_include, &zlib_library)?;

    Ok(NativeLibraries {
        flatbuffers_include,
        flatc,
        mysofa_include,
        mysofa_library,
        pffft_include,
        pffft_library,
        zlib_include,
        zlib_library,
    })
}

fn base_config(source: &Path, output: &Path, profile: &str) -> cmake::Config {
    let mut config = cmake::Config::new(source);
    config
        .out_dir(output)
        .profile(profile)
        .define("CMAKE_POLICY_VERSION_MINIMUM", "3.5")
        .define("CMAKE_POSITION_INDEPENDENT_CODE", "ON");
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
        "MultiThreaded$<$<CONFIG:Debug>:Debug>"
    } else {
        "MultiThreaded$<$<CONFIG:Debug>:Debug>DLL"
    }
}

fn build_flatbuffers(out_dir: &Path, profile: &str) -> Result<(PathBuf, PathBuf), Box<dyn Error>> {
    let host = env::var("HOST")?;
    let target = env::var("TARGET")?;
    if host != target {
        let flatc = env::var_os("BLACKFLOWER_FLATC").ok_or(
            "cross-compiling blackflower-audio requires BLACKFLOWER_FLATC to name a \
             host executable built from the pinned FlatBuffers source",
        )?;
        return Ok((
            PathBuf::from(FLATBUFFERS_ROOT).join("include"),
            PathBuf::from(flatc),
        ));
    }

    let source = stage_source(Path::new(FLATBUFFERS_ROOT), out_dir, "flatbuffers")?;
    let output = out_dir.join("native/flatbuffers");
    let mut config = base_config(&source, &output, profile);
    config
        .build_target("flatc")
        .define("FLATBUFFERS_BUILD_TESTS", "OFF")
        .define("FLATBUFFERS_BUILD_FLATLIB", "OFF")
        .define("FLATBUFFERS_BUILD_FLATC", "ON")
        .define("FLATBUFFERS_BUILD_FLATHASH", "OFF")
        .define("FLATBUFFERS_BUILD_GRPCTEST", "OFF")
        .define("FLATBUFFERS_BUILD_SHAREDLIB", "OFF");
    let destination = config.build();
    let executable = if cfg!(windows) { "flatc.exe" } else { "flatc" };
    let flatc = find_built_file(&destination, executable)?;
    Ok((source.join("include"), flatc))
}

fn build_zlib(out_dir: &Path, profile: &str) -> Result<(PathBuf, PathBuf), Box<dyn Error>> {
    let source = stage_source(Path::new(ZLIB_ROOT), out_dir, "zlib")?;
    let output = out_dir.join("native/zlib");
    let mut config = base_config(&source, &output, profile);
    config
        .build_target("zlibstatic")
        .define("BUILD_SHARED_LIBS", "OFF")
        .define("ZLIB_BUILD_EXAMPLES", "OFF");
    let destination = config.build();
    let library = find_static_library(&destination, "z", "zlibstatic")?;
    let generated_header = output.join("build/zconf.h");
    require_path(&generated_header)?;
    let include = output.join("static-include");
    fs::create_dir_all(&include)?;
    fs::copy(source.join("zlib.h"), include.join("zlib.h"))?;
    fs::copy(generated_header, include.join("zconf.h"))?;
    Ok((include, library))
}

fn build_pffft(out_dir: &Path, profile: &str) -> Result<(PathBuf, PathBuf), Box<dyn Error>> {
    let source = stage_source(Path::new(PFFFT_ROOT), out_dir, "pffft")?;
    let output = out_dir.join("native/pffft");
    let mut config = base_config(&source, &output, profile);
    config
        .build_target("PFFFT")
        .define("PFFFT_USE_TYPE_DOUBLE", "OFF")
        .define("PFFFT_USE_BENCH_GREEN", "OFF")
        .define("PFFFT_USE_BENCH_KISS", "OFF")
        .define("PFFFT_USE_BENCH_POCKET", "OFF")
        .define("PFFFT_USE_FFTPACK", "OFF");
    let destination = config.build();
    let library = find_static_library(&destination, "pffft", "pffft")?;
    Ok((source, library))
}

fn build_mysofa(
    out_dir: &Path,
    profile: &str,
    zlib_include: &Path,
    zlib_library: &Path,
) -> Result<(PathBuf, PathBuf), Box<dyn Error>> {
    let source = stage_source(Path::new(MYSOFA_ROOT), out_dir, "libmysofa")?;
    patch_mysofa_zlib_discovery(&source)?;
    let output = out_dir.join("native/libmysofa");
    let mut config = base_config(&source, &output, profile);
    config
        .build_target("mysofa-static")
        .define("BUILD_TESTS", "OFF")
        .define("BUILD_SHARED_LIBS", "OFF")
        .define("BUILD_STATIC_LIBS", "ON")
        .define("ZLIB_INCLUDE_DIR", zlib_include)
        .define("ZLIB_LIBRARY", zlib_library);
    let destination = config.build();
    let library = find_static_library(&destination, "mysofa", "mysofa")?;
    let export_header = output.join("build/src/mysofa_export.h");
    require_path(&export_header)?;
    let include = output.join("static-include");
    fs::create_dir_all(&include)?;
    fs::copy(source.join("src/hrtf/mysofa.h"), include.join("mysofa.h"))?;
    fs::copy(export_header, include.join("mysofa_export.h"))?;
    Ok((include, library))
}

fn patch_mysofa_zlib_discovery(source: &Path) -> Result<(), Box<dyn Error>> {
    let cmake_path = source.join("src/CMakeLists.txt");
    let contents = fs::read_to_string(&cmake_path)?;
    let start = contents
        .find("if(NOT MSVC)")
        .ok_or("libmysofa CMake zlib discovery start was not found")?;
    let end = contents[start..]
        .find("set(libsrc")
        .map(|offset| start + offset)
        .ok_or("libmysofa CMake zlib discovery end was not found")?;
    let replacement = "\
if(NOT WIN32)\n\
  find_library(MATH m)\n\
else()\n\
  set(MATH \"\")\n\
endif()\n\
find_package(ZLIB REQUIRED)\n\
include_directories(${ZLIB_INCLUDE_DIRS})\n\
set(PKG_CONFIG_PRIVATELIBS \"-lm -lz ${PKG_CONFIG_PRIVATELIBS}\")\n\
\n";
    let mut patched = contents;
    patched.replace_range(start..end, replacement);
    fs::remove_file(&cmake_path)?;
    fs::write(cmake_path, patched)?;
    Ok(())
}

fn build_steam_audio(
    out_dir: &Path,
    profile: &str,
    libraries: &NativeLibraries,
) -> Result<PathBuf, Box<dyn Error>> {
    let source = stage_source(Path::new(SDK_ROOT), out_dir, "steam-audio")?;
    patch_steam_audio_linux_abi(&source)?;
    let output = out_dir.join("native/steam-audio");
    let mut config = base_config(&source, &output, profile);
    config
        .build_target("phonon")
        .define("BUILD_SHARED_LIBS", "OFF")
        .define("STEAMAUDIO_STATIC_RUNTIME", static_crt_setting())
        .define("STEAMAUDIO_BUILD_TESTS", "OFF")
        .define("STEAMAUDIO_BUILD_ITESTS", "OFF")
        .define("STEAMAUDIO_BUILD_BENCHMARKS", "OFF")
        .define("STEAMAUDIO_BUILD_SAMPLES", "OFF")
        .define("STEAMAUDIO_BUILD_DOCS", "OFF")
        .define("STEAMAUDIO_ENABLE_IPP", "OFF")
        .define("STEAMAUDIO_ENABLE_MKL", "OFF")
        .define("STEAMAUDIO_ENABLE_EMBREE", "OFF")
        .define("STEAMAUDIO_ENABLE_FFTS", "OFF")
        .define("STEAMAUDIO_ENABLE_RADEONRAYS", "OFF")
        .define("STEAMAUDIO_ENABLE_TRUEAUDIONEXT", "OFF")
        .define("FlatBuffers_INCLUDE_DIR", &libraries.flatbuffers_include)
        .define("FlatBuffers_EXECUTABLE", &libraries.flatc)
        .define("PFFFT_INCLUDE_DIR", &libraries.pffft_include)
        .define("PFFFT_LIBRARY", &libraries.pffft_library)
        .define("MySOFA_INCLUDE_DIR", &libraries.mysofa_include)
        .define("MySOFA_LIBRARY", &libraries.mysofa_library)
        .define("ZLIB_INCLUDE_DIR", &libraries.zlib_include)
        .define("ZLIB_LIBRARY", &libraries.zlib_library);
    let destination = config.build();
    find_static_library(&destination, "phonon", "phonon")
}

fn patch_steam_audio_linux_abi(source: &Path) -> Result<(), Box<dyn Error>> {
    if env::var("CARGO_CFG_TARGET_OS")? != "linux" {
        return Ok(());
    }

    const LEGACY_ABI_OPTION: &str = "        add_compile_options(-fabi-version=6)\n";
    let cmake_path = source.join("CMakeLists.txt");
    let contents = fs::read_to_string(&cmake_path)?;
    let occurrences = contents.matches(LEGACY_ABI_OPTION).count();
    if occurrences != 1 {
        return Err(
            format!("expected one Steam Audio legacy GCC ABI option, found {occurrences}").into(),
        );
    }

    fs::remove_file(&cmake_path)?;
    fs::write(cmake_path, contents.replacen(LEGACY_ABI_OPTION, "", 1))?;
    Ok(())
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

fn find_static_library(
    root: &Path,
    unix_name: &str,
    windows_name: &str,
) -> Result<PathBuf, Box<dyn Error>> {
    if target_is_windows() {
        let file_name = format!("{windows_name}.lib");
        find_built_file(root, &file_name).or_else(|_error| {
            let debug_file_name = format!("{windows_name}d.lib");
            find_built_file(root, &debug_file_name)
        })
    } else {
        find_built_file(root, &format!("lib{unix_name}.a"))
    }
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

fn emit_static_linking(phonon: &Path, libraries: &NativeLibraries) -> Result<(), Box<dyn Error>> {
    for library in [
        phonon,
        &libraries.pffft_library,
        &libraries.mysofa_library,
        &libraries.zlib_library,
    ] {
        let directory = library
            .parent()
            .ok_or_else(|| format!("{} has no parent directory", library.display()))?;
        let name = static_link_name(library)?;
        println!("cargo:rustc-link-search=native={}", directory.display());
        println!("cargo:rustc-link-lib=static={name}");
    }

    let target_os = env::var("CARGO_CFG_TARGET_OS")?;
    match target_os.as_str() {
        "linux" => {
            println!("cargo:rustc-link-lib=dylib=stdc++");
            println!("cargo:rustc-link-lib=dylib=m");
            println!("cargo:rustc-link-lib=dylib=dl");
            println!("cargo:rustc-link-lib=dylib=pthread");
        }
        "macos" => println!("cargo:rustc-link-lib=dylib=c++"),
        "windows" => println!("cargo:rustc-link-lib=dylib=delayimp"),
        _ => {}
    }
    Ok(())
}

fn static_link_name(library: &Path) -> Result<&str, Box<dyn Error>> {
    let stem = library
        .file_stem()
        .and_then(OsStr::to_str)
        .ok_or_else(|| format!("invalid static library name {}", library.display()))?;
    Ok(stem.strip_prefix("lib").unwrap_or(stem))
}
