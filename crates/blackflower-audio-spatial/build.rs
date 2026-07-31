use std::env;
use std::error::Error;
use std::ffi::OsStr;
use std::fs;
use std::panic::{AssertUnwindSafe, catch_unwind};
use std::path::{Path, PathBuf};
use std::process::Command;

const EXPECTED_SDK_VERSION: &str = "4.8.1";
const EXPECTED_EMBREE_VERSION: &str = "4.4.1";
const EXPECTED_ISPC_VERSION: &str = "1.31.0";
const SDK_ROOT: &str = "vendor/steam-audio-sdk/core";
const SDK_BUILD: &str = "vendor/steam-audio-sdk/core/CMakeLists.txt";
const SDK_INCLUDE: &str = "vendor/steam-audio-sdk/core/src/core";
const SDK_VERSION_TEMPLATE: &str = "vendor/steam-audio-sdk/core/src/core/phonon_version.h.in";
const EMBREE_ROOT: &str = "vendor/embree";
const EMBREE_BUILD: &str = "vendor/embree/CMakeLists.txt";
const FLATBUFFERS_ROOT: &str = "vendor/flatbuffers";
const MYSOFA_ROOT: &str = "vendor/libmysofa";
const PFFFT_ROOT: &str = "vendor/pffft";
const ZLIB_ROOT: &str = "../../vendor/zlib";
const WRAPPER_HEADER: &str = "native/wrapper.h";

struct NativeLibraries {
    embree: Option<EmbreeLibraries>,
    flatbuffers_include: PathBuf,
    flatc: PathBuf,
    mysofa_include: PathBuf,
    mysofa_library: PathBuf,
    pffft_include: PathBuf,
    pffft_library: PathBuf,
    zlib_include: PathBuf,
    zlib_library: PathBuf,
}

#[allow(
    dead_code,
    reason = "Cargo's all-target build-script lint pass does not observe the native link consumers"
)]
struct EmbreeLibraries {
    include: PathBuf,
    ispc: Option<PathBuf>,
    lexers: PathBuf,
    math: PathBuf,
    simd: PathBuf,
    sys: PathBuf,
    tasking: PathBuf,
    sse2: PathBuf,
    sse4: Option<PathBuf>,
    avx: Option<PathBuf>,
    avx2: Option<PathBuf>,
}

struct SteamAudioLibraries {
    phonon: PathBuf,
    ispc_kernels: Option<PathBuf>,
}

fn main() -> Result<(), Box<dyn Error>> {
    println!("cargo:rustc-check-cfg=cfg(blackflower_steam_audio_embree)");
    emit_rebuild_inputs()?;
    let out_dir = PathBuf::from(env::var_os("OUT_DIR").ok_or("OUT_DIR is not set")?);
    let profile = native_profile();
    verify_sdk_version()?;
    generate_version_header(&out_dir)?;
    generate_bindings(&out_dir)?;

    let libraries = build_native_libraries(&out_dir, profile)?;
    if libraries.embree.is_some() {
        println!("cargo:rustc-cfg=blackflower_steam_audio_embree");
    }
    let steam_audio = build_steam_audio(&out_dir, profile, &libraries)?;
    emit_static_linking(&steam_audio, &libraries)?;
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
    if embree_supported_target()? {
        println!("cargo:rerun-if-changed={EMBREE_ROOT}");
        require_path(Path::new(EMBREE_ROOT))?;
    }
    println!("cargo:rerun-if-env-changed=BLACKFLOWER_FLATC");
    println!("cargo:rerun-if-env-changed=BLACKFLOWER_ISPC");
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
        "RelWithDebInfo"
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
    let embree = if embree_supported_target()? {
        Some(build_embree(out_dir, profile)?)
    } else {
        None
    };
    let (flatbuffers_include, flatc) = build_flatbuffers(out_dir, profile)?;
    let (zlib_include, zlib_library) = build_zlib(out_dir, profile)?;
    let (pffft_include, pffft_library) = build_pffft(out_dir, profile)?;
    let (mysofa_include, mysofa_library) =
        build_mysofa(out_dir, profile, &zlib_include, &zlib_library)?;

    Ok(NativeLibraries {
        embree,
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

fn embree_supported_target() -> Result<bool, Box<dyn Error>> {
    let architecture = env::var("CARGO_CFG_TARGET_ARCH")?;
    let operating_system = env::var("CARGO_CFG_TARGET_OS")?;
    Ok((architecture == "x86_64"
        && matches!(operating_system.as_str(), "linux" | "macos" | "windows"))
        || (architecture == "aarch64" && matches!(operating_system.as_str(), "linux" | "macos")))
}

#[allow(
    clippy::too_many_lines,
    reason = "the pinned Embree build contract keeps every CMake option and output check together"
)]
fn build_embree(out_dir: &Path, profile: &str) -> Result<EmbreeLibraries, Box<dyn Error>> {
    verify_embree_version()?;
    let architecture = env::var("CARGO_CFG_TARGET_ARCH")?;
    let ispc = (architecture == "x86_64").then(find_ispc).transpose()?;
    let source = stage_source(Path::new(EMBREE_ROOT), out_dir, "embree")?;
    let output = out_dir.join("native/embree");
    let mut config = base_config(&source, &output, profile);
    config
        .build_target("install")
        .define("BUILD_TESTING", "OFF")
        .define("EMBREE_STATIC_LIB", "ON")
        .define("EMBREE_STATIC_RUNTIME", static_crt_setting())
        .define(
            "EMBREE_ISPC_SUPPORT",
            if ispc.is_some() { "ON" } else { "OFF" },
        )
        .define("EMBREE_TUTORIALS", "OFF")
        .define("EMBREE_GEOMETRY_TRIANGLE", "ON")
        .define("EMBREE_GEOMETRY_QUAD", "OFF")
        .define("EMBREE_GEOMETRY_CURVE", "OFF")
        .define("EMBREE_GEOMETRY_SUBDIVISION", "OFF")
        .define("EMBREE_GEOMETRY_USER", "OFF")
        .define("EMBREE_GEOMETRY_INSTANCE", "ON")
        .define("EMBREE_GEOMETRY_INSTANCE_ARRAY", "OFF")
        .define("EMBREE_GEOMETRY_GRID", "OFF")
        .define("EMBREE_GEOMETRY_POINT", "OFF")
        .define("EMBREE_TASKING_SYSTEM", "INTERNAL")
        .define("EMBREE_LIBRARY_NAME", "embree");
    if let Some(ispc) = &ispc {
        config.define("EMBREE_ISPC_EXECUTABLE", ispc);
    }
    if architecture == "aarch64" {
        config.define("EMBREE_MAX_ISA", "NONE");
    } else if env::var_os("CARGO_CFG_TARGET_OS").as_deref() == Some(OsStr::new("macos")) {
        config
            .define("EMBREE_ISA_SSE2", "ON")
            .define("EMBREE_ISA_SSE42", "OFF")
            .define("EMBREE_ISA_AVX", "OFF")
            .define("EMBREE_ISA_AVX2", "OFF")
            .define("EMBREE_ISA_AVX512", "OFF");
    } else {
        config.define("EMBREE_MAX_ISA", "AVX2");
    }
    let destination = config.build();
    let include = destination.join("include/embree4");
    require_path(&include.join("rtcore.h"))?;
    let has_x86_isa_variants = architecture == "x86_64"
        && env::var_os("CARGO_CFG_TARGET_OS").as_deref() != Some(OsStr::new("macos"));
    let has_avx2_named_variant = has_x86_isa_variants
        || (architecture == "aarch64"
            && env::var_os("CARGO_CFG_TARGET_OS").as_deref() == Some(OsStr::new("macos")));
    Ok(EmbreeLibraries {
        include,
        ispc,
        lexers: find_static_library(&destination, "lexers", "lexers")?,
        math: find_static_library(&destination, "math", "math")?,
        simd: find_static_library(&destination, "simd", "simd")?,
        sys: find_static_library(&destination, "sys", "sys")?,
        tasking: find_static_library(&destination, "tasking", "tasking")?,
        sse2: find_static_library(&destination, "embree", "embree")?,
        sse4: has_x86_isa_variants
            .then(|| find_static_library(&destination, "embree_sse42", "embree_sse42"))
            .transpose()?,
        avx: has_x86_isa_variants
            .then(|| find_static_library(&destination, "embree_avx", "embree_avx"))
            .transpose()?,
        // Embree names its Apple NEON2X archive `embree_avx2` internally.
        avx2: has_avx2_named_variant
            .then(|| find_static_library(&destination, "embree_avx2", "embree_avx2"))
            .transpose()?,
    })
}

fn verify_embree_version() -> Result<(), Box<dyn Error>> {
    let build = fs::read_to_string(EMBREE_BUILD)?;
    let component = |name: &str| {
        build.lines().find_map(|line| {
            line.trim()
                .strip_prefix(&format!("SET(EMBREE_VERSION_{name} "))
                .and_then(|value| value.strip_suffix(')'))
        })
    };
    let version = format!(
        "{}.{}.{}",
        component("MAJOR").ok_or("Embree has no major version")?,
        component("MINOR").ok_or("Embree has no minor version")?,
        component("PATCH").ok_or("Embree has no patch version")?
    );
    if version == EXPECTED_EMBREE_VERSION {
        Ok(())
    } else {
        Err(
            format!("Embree submodule version is {version}; expected {EXPECTED_EMBREE_VERSION}")
                .into(),
        )
    }
}

fn find_ispc() -> Result<PathBuf, Box<dyn Error>> {
    let executable = env::var_os("BLACKFLOWER_ISPC")
        .map(PathBuf::from)
        .or_else(|| {
            env::var_os("PATH").and_then(|path| {
                env::split_paths(&path)
                    .map(|directory| {
                        directory.join(if cfg!(windows) { "ispc.exe" } else { "ispc" })
                    })
                    .find(|candidate| candidate.is_file())
            })
        })
        .ok_or("Embree support requires ISPC 1.31.0; set BLACKFLOWER_ISPC to its executable")?;
    let output = Command::new(&executable).arg("--version").output()?;
    let version = String::from_utf8_lossy(&output.stdout);
    if output.status.success() && version.contains(EXPECTED_ISPC_VERSION) {
        Ok(executable)
    } else {
        Err(format!(
            "{} is not ISPC {EXPECTED_ISPC_VERSION}: {}",
            executable.display(),
            version.trim()
        )
        .into())
    }
}

fn base_config(source: &Path, output: &Path, profile: &str) -> cmake::Config {
    let mut config = cmake::Config::new(source);
    config
        .out_dir(output)
        .profile(profile)
        .define("CMAKE_POLICY_VERSION_MINIMUM", "3.5")
        .define("CMAKE_POSITION_INDEPENDENT_CODE", "ON");
    if env::var_os("CARGO_CFG_TARGET_OS").as_deref() == Some(OsStr::new("macos"))
        && let Ok(architecture) = env::var("CARGO_CFG_TARGET_ARCH")
    {
        let (cmake_architecture, deployment_target) = match architecture.as_str() {
            "aarch64" => ("arm64", "11.0"),
            "x86_64" => ("x86_64", "10.13"),
            _ => (architecture.as_str(), "10.13"),
        };
        config
            .define("CMAKE_OSX_ARCHITECTURES", cmake_architecture)
            .define("CMAKE_OSX_DEPLOYMENT_TARGET", deployment_target);
    }
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
            "cross-compiling blackflower-audio-spatial requires BLACKFLOWER_FLATC to name a \
             host executable built from the pinned FlatBuffers source",
        )?;
        return Ok((
            fs::canonicalize(Path::new(FLATBUFFERS_ROOT).join("include"))?,
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

#[allow(
    clippy::too_many_lines,
    reason = "the pinned Steam Audio build contract keeps every CMake option and Embree binding together"
)]
fn build_steam_audio(
    out_dir: &Path,
    profile: &str,
    libraries: &NativeLibraries,
) -> Result<SteamAudioLibraries, Box<dyn Error>> {
    let source = stage_source(Path::new(SDK_ROOT), out_dir, "steam-audio")?;
    patch_steam_audio_linux_abi(&source)?;
    if libraries.embree.is_some() {
        patch_steam_audio_embree_include(&source)?;
        patch_steam_audio_embree_scene_loading(&source)?;
        patch_steam_audio_ispc_version(&source)?;
        patch_steam_audio_embree_arm64(&source)?;
    }
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
        .define(
            "STEAMAUDIO_ENABLE_EMBREE",
            if libraries.embree.is_some() {
                "ON"
            } else {
                "OFF"
            },
        )
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
    if let Some(embree) = &libraries.embree {
        config
            .define("Embree_INCLUDE_DIR", &embree.include)
            .define("Embree_lexers_LIBRARY", &embree.lexers)
            .define("Embree_math_LIBRARY", &embree.math)
            .define("Embree_simd_LIBRARY", &embree.simd)
            .define("Embree_sys_LIBRARY", &embree.sys)
            .define("Embree_tasking_LIBRARY", &embree.tasking)
            .define("Embree_sse2_LIBRARY", &embree.sse2);
        if let Some(ispc) = &embree.ispc {
            config
                .define("ISPC_EXECUTABLE", ispc)
                .define("ISPC_VERSION", EXPECTED_ISPC_VERSION);
        }
        if env::var("CARGO_CFG_TARGET_ARCH")? == "aarch64" {
            config.define("BLACKFLOWER_EMBREE_CPP_REFLECTION", "ON");
        }
        if let Some(library) = &embree.sse4 {
            config.define("Embree_sse4_LIBRARY", library);
        }
        if let Some(library) = &embree.avx {
            config.define("Embree_avx_LIBRARY", library);
        }
        if let Some(library) = &embree.avx2 {
            config.define("Embree_avx2_LIBRARY", library);
        }
    }
    let destination = config.build();
    Ok(SteamAudioLibraries {
        phonon: find_static_library(&destination, "phonon", "phonon")?,
        ispc_kernels: libraries
            .embree
            .as_ref()
            .and_then(|embree| embree.ispc.as_ref())
            .map(|_ispc| find_static_library(&destination, "ispckernels", "ispckernels"))
            .transpose()?,
    })
}

fn patch_steam_audio_ispc_version(source: &Path) -> Result<(), Box<dyn Error>> {
    replace_exact(
        &source.join("CMakeLists.txt"),
        "find_package(ISPC 1.12 EXACT)",
        "find_package(ISPC 1.31 EXACT)",
        "Steam Audio ISPC version contract changed",
    )
}

fn patch_steam_audio_embree_include(source: &Path) -> Result<(), Box<dyn Error>> {
    const ORIGINAL: &str =
        "    set(ISPC_FLAGS          -I ${CMAKE_HOME_DIRECTORY}/deps/embree/include -g)";
    const REPLACEMENT: &str = "    set(ISPC_FLAGS          -I ${Embree_INCLUDE_DIR} -g)";
    let cmake_path = source.join("src/core/CMakeLists.txt");
    let contents = fs::read_to_string(&cmake_path)?;
    if contents.matches(ORIGINAL).count() != 1 {
        return Err("Steam Audio Embree ISPC include path contract changed".into());
    }
    fs::remove_file(&cmake_path)?;
    fs::write(&cmake_path, contents.replacen(ORIGINAL, REPLACEMENT, 1))?;

    const DEVICE_ORIGINAL: &str = "    mDevice = rtcNewDevice(nullptr);";
    const DEVICE_REPLACEMENT: &str = "    mDevice = rtcNewDevice(\"set_affinity=0\");";
    let device_path = source.join("src/core/embree_device.cpp");
    let contents = fs::read_to_string(&device_path)?;
    if contents.matches(DEVICE_ORIGINAL).count() != 1 {
        return Err("Steam Audio Embree device configuration contract changed".into());
    }
    fs::remove_file(&device_path)?;
    fs::write(
        &device_path,
        contents.replacen(DEVICE_ORIGINAL, DEVICE_REPLACEMENT, 1),
    )?;
    Ok(())
}

#[allow(
    clippy::too_many_lines,
    reason = "the pinned-source patch keeps every exact Steam Audio scene-loading contract together"
)]
fn patch_steam_audio_embree_scene_loading(source: &Path) -> Result<(), Box<dyn Error>> {
    let header_path = source.join("src/core/embree_static_mesh.h");
    replace_exact(
        &header_path,
        r#"    EmbreeStaticMesh(shared_ptr<EmbreeScene> scene,
                     const Serialized::StaticMesh* serializedObject);

    EmbreeStaticMesh(shared_ptr<EmbreeScene> scene,
                     SerializedObject& serializedObject);"#,
        r#"    EmbreeStaticMesh(shared_ptr<EmbreeScene> scene,
                     const Serialized::StaticMesh* serializedObject);

    // A deserialized scene owns this mesh directly, before shared_from_this() is available.
    EmbreeStaticMesh(EmbreeScene& scene,
                     const Serialized::StaticMesh* serializedObject);

    EmbreeStaticMesh(shared_ptr<EmbreeScene> scene,
                     SerializedObject& serializedObject);"#,
        "Steam Audio Embree static mesh constructor contract changed",
    )?;
    replace_exact(
        &header_path,
        r#"    void initialize(const EmbreeScene& scene,
                    const Vector3f* vertices,
                    const Triangle* triangles);

    void convertMaterials();

    std::weak_ptr<EmbreeScene> mScene;
    RTCGeometry mGeometry;"#,
        r#"    void initialize(const EmbreeScene& scene,
                    const Vector3f* vertices,
                    const Triangle* triangles);

    void initialize(const EmbreeScene& scene,
                    const Serialized::StaticMesh* serializedObject);

    void convertMaterials();

    std::weak_ptr<EmbreeScene> mScene;
    EmbreeScene* mOwningScene = nullptr;
    RTCGeometry mGeometry;"#,
        "Steam Audio Embree static mesh member contract changed",
    )?;

    let static_mesh_path = source.join("src/core/embree_static_mesh.cpp");
    replace_exact(
        &static_mesh_path,
        r#"EmbreeStaticMesh::EmbreeStaticMesh(shared_ptr<EmbreeScene> scene,
                                   const Serialized::StaticMesh* serializedObject)
    : mScene(scene)
{
    assert(serializedObject);"#,
        r#"EmbreeStaticMesh::EmbreeStaticMesh(shared_ptr<EmbreeScene> scene,
                                   const Serialized::StaticMesh* serializedObject)
    : mScene(scene)
{
    initialize(*scene, serializedObject);
}

EmbreeStaticMesh::EmbreeStaticMesh(EmbreeScene& scene,
                                   const Serialized::StaticMesh* serializedObject)
    : mOwningScene(&scene)
{
    initialize(scene, serializedObject);
}

void EmbreeStaticMesh::initialize(const EmbreeScene& scene,
                                  const Serialized::StaticMesh* serializedObject)
{
    assert(serializedObject);"#,
        "Steam Audio Embree serialized mesh constructor contract changed",
    )?;
    replace_exact(
        &static_mesh_path,
        "    initialize(*scene, vertices.data(), triangles.data());",
        "    initialize(scene, vertices.data(), triangles.data());",
        "Steam Audio Embree serialized mesh initialization contract changed",
    )?;
    replace_exact(
        &static_mesh_path,
        r#"EmbreeStaticMesh::~EmbreeStaticMesh()
{
    if (auto scene = mScene.lock())
    {
        scene->releaseGeometryID(mGeometryIndex);
        rtcReleaseGeometry(mGeometry);
    }
}"#,
        r#"EmbreeStaticMesh::~EmbreeStaticMesh()
{
    if (auto scene = mScene.lock())
    {
        scene->releaseGeometryID(mGeometryIndex);
    }
    else if (mOwningScene)
    {
        mOwningScene->releaseGeometryID(mGeometryIndex);
    }

    rtcReleaseGeometry(mGeometry);
}"#,
        "Steam Audio Embree static mesh destructor contract changed",
    )?;

    let scene_path = source.join("src/core/embree_scene.cpp");
    replace_exact(
        &scene_path,
        "        auto staticMesh = ipl::make_shared<EmbreeStaticMesh>(std::static_pointer_cast<EmbreeScene>(shared_from_this()), serializedObject->static_meshes()->Get(i));",
        "        auto staticMesh = ipl::make_shared<EmbreeStaticMesh>(*this, serializedObject->static_meshes()->Get(i));",
        "Steam Audio Embree serialized scene construction contract changed",
    )?;
    replace_exact(
        &scene_path,
        r#"EmbreeScene::~EmbreeScene()
{
    rtcReleaseScene(mScene);
}"#,
        r#"EmbreeScene::~EmbreeScene()
{
    // Scene-owned deserialized meshes must be destroyed while the parent is still valid.
    mStaticMeshes[0].clear();
    mStaticMeshes[1].clear();
    rtcReleaseScene(mScene);
}"#,
        "Steam Audio Embree scene destructor contract changed",
    )?;
    Ok(())
}

#[allow(
    clippy::too_many_lines,
    reason = "the pinned-source patch keeps Steam Audio's ARM64 Embree port auditable"
)]
fn patch_steam_audio_embree_arm64(source: &Path) -> Result<(), Box<dyn Error>> {
    if env::var("CARGO_CFG_TARGET_ARCH")? != "aarch64" {
        return Ok(());
    }

    const X86_GUARD: &str =
        "#if defined(IPL_USES_EMBREE) && (defined(IPL_CPU_X86) || defined(IPL_CPU_X64))";
    const ARM64_GUARD: &str = "#if defined(IPL_USES_EMBREE) && (defined(IPL_CPU_X86) || defined(IPL_CPU_X64) || defined(IPL_CPU_ARM64))";
    let core = source.join("src/core");
    for (file, expected) in [
        ("api_embree_device.cpp", 4),
        ("embree_device.cpp", 1),
        ("embree_device.h", 1),
        ("embree_instanced_mesh.cpp", 1),
        ("embree_instanced_mesh.h", 1),
        ("embree_scene.cpp", 1),
        ("embree_scene.h", 1),
        ("embree_static_mesh.cpp", 1),
        ("embree_static_mesh.h", 1),
        ("pch.h", 1),
        ("scene_factory.cpp", 2),
    ] {
        replace_all_checked(
            &core.join(file),
            X86_GUARD,
            ARM64_GUARD,
            expected,
            "Steam Audio Embree architecture guard contract changed",
        )?;
    }

    let root_cmake = source.join("CMakeLists.txt");
    replace_exact(
        &root_cmake,
        "    set(CMAKE_OSX_ARCHITECTURES \"x86_64;arm64\")\n    set(CMAKE_OSX_DEPLOYMENT_TARGET \"10.13\")",
        "    # The embedding build supplies one target architecture and deployment target.",
        "Steam Audio macOS target contract changed",
    )?;
    replace_exact(
        &root_cmake,
        r#"if (STEAMAUDIO_ENABLE_EMBREE)
    find_package(ISPC 1.31 EXACT)
    find_package(Embree 4)
    if (NOT ISPC_FOUND OR NOT Embree_FOUND)
        message(STATUS "Disabling Embree")
        set(STEAMAUDIO_ENABLE_EMBREE OFF)
    endif()
endif()"#,
        r#"if (STEAMAUDIO_ENABLE_EMBREE)
    if (NOT BLACKFLOWER_EMBREE_CPP_REFLECTION)
        find_package(ISPC 1.31 EXACT)
    endif()
    find_package(Embree 4)
    if ((NOT BLACKFLOWER_EMBREE_CPP_REFLECTION AND NOT ISPC_FOUND) OR NOT Embree_FOUND)
        message(STATUS "Disabling Embree")
        set(STEAMAUDIO_ENABLE_EMBREE OFF)
    endif()
endif()"#,
        "Steam Audio Embree dependency discovery contract changed",
    )?;

    let core_cmake = core.join("CMakeLists.txt");
    replace_exact(
        &core_cmake,
        "if (STEAMAUDIO_ENABLE_EMBREE)\n    if (WIN32)",
        "if (STEAMAUDIO_ENABLE_EMBREE AND NOT BLACKFLOWER_EMBREE_CPP_REFLECTION)\n    if (WIN32)",
        "Steam Audio Embree ISPC build contract changed",
    )?;
    replace_exact(
        &core_cmake,
        r#"if (STEAMAUDIO_ENABLE_EMBREE)
    target_sources(core PRIVATE
        embree_device.h
        embree_device.cpp
        embree_static_mesh.h
        embree_static_mesh.cpp
        embree_instanced_mesh.h
        embree_instanced_mesh.cpp
        embree_scene.h
        embree_scene.cpp
        embree_reflection_simulator.h
        embree_reflection_simulator.cpp
        embree_reflection_simulator.ispc
    )
endif()"#,
        r#"if (STEAMAUDIO_ENABLE_EMBREE)
    target_sources(core PRIVATE
        embree_device.h
        embree_device.cpp
        embree_static_mesh.h
        embree_static_mesh.cpp
        embree_instanced_mesh.h
        embree_instanced_mesh.cpp
        embree_scene.h
        embree_scene.cpp
    )
    if (NOT BLACKFLOWER_EMBREE_CPP_REFLECTION)
        target_sources(core PRIVATE
            embree_reflection_simulator.h
            embree_reflection_simulator.cpp
            embree_reflection_simulator.ispc
        )
    endif()
endif()"#,
        "Steam Audio Embree source list contract changed",
    )?;
    replace_exact(
        &core_cmake,
        r#"if (STEAMAUDIO_ENABLE_EMBREE)
    target_link_libraries(core PUBLIC Embree::Embree ispckernels)
endif()"#,
        r#"if (STEAMAUDIO_ENABLE_EMBREE)
    target_link_libraries(core PUBLIC Embree::Embree)
    if (NOT BLACKFLOWER_EMBREE_CPP_REFLECTION)
        target_link_libraries(core PUBLIC ispckernels)
    endif()
endif()"#,
        "Steam Audio Embree link contract changed",
    )?;

    let find_embree = source.join("build/FindEmbree.cmake");
    replace_all_checked(
        &find_embree,
        "if (NOT IPL_OS_MACOS)",
        "if (NOT IPL_OS_MACOS AND NOT IPL_CPU_ARMV8)",
        3,
        "Steam Audio Embree ISA library discovery contract changed",
    )?;
    replace_all_checked(
        &find_embree,
        "if (IPL_OS_MACOS)",
        "if (IPL_OS_MACOS OR IPL_CPU_ARMV8)",
        2,
        "Steam Audio Embree base library contract changed",
    )?;

    let scene_header = core.join("embree_scene.h");
    replace_exact(
        &scene_header,
        "#include \"embree_reflection_simulator.ispc.h\"",
        "#if defined(IPL_CPU_X86) || defined(IPL_CPU_X64)\n#include \"embree_reflection_simulator.ispc.h\"\n#endif",
        "Steam Audio Embree scene ISPC include contract changed",
    )?;
    replace_exact(
        &scene_header,
        r#"    const ispc::Material* const* ispcMaterialsForGeometry() const
    {
        return mISPCMaterialsForGeometry.data();
    }
"#,
        r#"#if defined(IPL_CPU_X86) || defined(IPL_CPU_X64)
    const ispc::Material* const* ispcMaterialsForGeometry() const
    {
        return mISPCMaterialsForGeometry.data();
    }
#endif
"#,
        "Steam Audio Embree scene ISPC accessor contract changed",
    )?;
    replace_exact(
        &scene_header,
        "    vector<const ispc::Material*> mISPCMaterialsForGeometry;",
        "#if defined(IPL_CPU_X86) || defined(IPL_CPU_X64)\n    vector<const ispc::Material*> mISPCMaterialsForGeometry;\n#endif",
        "Steam Audio Embree scene ISPC storage contract changed",
    )?;

    let static_mesh_header = core.join("embree_static_mesh.h");
    replace_exact(
        &static_mesh_header,
        r#"    ispc::Material* ispcMaterials()
    {
        return mISPCMaterials.data();
    }

    const ispc::Material* ispcMaterials() const
    {
        return mISPCMaterials.data();
    }
"#,
        r#"#if defined(IPL_CPU_X86) || defined(IPL_CPU_X64)
    ispc::Material* ispcMaterials()
    {
        return mISPCMaterials.data();
    }

    const ispc::Material* ispcMaterials() const
    {
        return mISPCMaterials.data();
    }
#endif
"#,
        "Steam Audio Embree static mesh ISPC accessor contract changed",
    )?;
    replace_exact(
        &static_mesh_header,
        "    vector<ispc::Material> mISPCMaterials;",
        "#if defined(IPL_CPU_X86) || defined(IPL_CPU_X64)\n    vector<ispc::Material> mISPCMaterials;\n#endif",
        "Steam Audio Embree static mesh ISPC storage contract changed",
    )?;

    let static_mesh = core.join("embree_static_mesh.cpp");
    replace_exact(
        &static_mesh,
        r#"void EmbreeStaticMesh::convertMaterials()
{
    mISPCMaterials.resize(mMaterials.size(0));

    for (auto i = 0; i < mMaterials.size(0); ++i)
    {
        mISPCMaterials[i].absorption = mMaterials[i].absorption;
        mISPCMaterials[i].scattering = mMaterials[i].scattering;
        mISPCMaterials[i].transmission = mMaterials[i].transmission;
    }
}"#,
        r#"void EmbreeStaticMesh::convertMaterials()
{
#if defined(IPL_CPU_X86) || defined(IPL_CPU_X64)
    mISPCMaterials.resize(mMaterials.size(0));

    for (auto i = 0; i < mMaterials.size(0); ++i)
    {
        mISPCMaterials[i].absorption = mMaterials[i].absorption;
        mISPCMaterials[i].scattering = mMaterials[i].scattering;
        mISPCMaterials[i].transmission = mMaterials[i].transmission;
    }
#endif
}"#,
        "Steam Audio Embree material conversion contract changed",
    )?;

    let scene = core.join("embree_scene.cpp");
    replace_exact(
        &scene,
        "    mISPCMaterialsForGeometry.resize(maxID + 1);",
        "#if defined(IPL_CPU_X86) || defined(IPL_CPU_X64)\n    mISPCMaterialsForGeometry.resize(maxID + 1);\n#endif",
        "Steam Audio Embree scene ISPC resize contract changed",
    )?;
    replace_all_checked(
        &scene,
        "        mISPCMaterialsForGeometry[index] = embreeStaticMesh->ispcMaterials();",
        "#if defined(IPL_CPU_X86) || defined(IPL_CPU_X64)\n        mISPCMaterialsForGeometry[index] = embreeStaticMesh->ispcMaterials();\n#endif",
        2,
        "Steam Audio Embree scene ISPC assignment contract changed",
    )?;

    let reflection_factory = core.join("reflection_simulator_factory.cpp");
    replace_exact(
        &reflection_factory,
        r#"#if defined(IPL_USES_EMBREE) && (defined(IPL_CPU_X86) || defined(IPL_CPU_X64))
    case SceneType::Embree:
        return ipl::make_unique<EmbreeReflectionSimulator>(maxNumRays, numDiffuseSamples, maxDuration, maxOrder, maxNumSources,
                                                           numThreads);
#endif"#,
        r#"#if defined(IPL_USES_EMBREE) && (defined(IPL_CPU_X86) || defined(IPL_CPU_X64))
    case SceneType::Embree:
        return ipl::make_unique<EmbreeReflectionSimulator>(maxNumRays, numDiffuseSamples, maxDuration, maxOrder, maxNumSources,
                                                           numThreads);
#elif defined(IPL_USES_EMBREE) && defined(IPL_CPU_ARM64)
    case SceneType::Embree:
        return ipl::make_unique<ReflectionSimulator>(maxNumRays, numDiffuseSamples, maxDuration, maxOrder, maxNumSources,
                                                     numThreads);
#endif"#,
        "Steam Audio Embree reflection simulator factory contract changed",
    )?;
    Ok(())
}

fn replace_exact(
    path: &Path,
    original: &str,
    replacement: &str,
    contract_error: &str,
) -> Result<(), Box<dyn Error>> {
    let contents = fs::read_to_string(path)?;
    if contents.matches(original).count() != 1 {
        return Err(contract_error.into());
    }
    fs::remove_file(path)?;
    fs::write(path, contents.replacen(original, replacement, 1))?;
    Ok(())
}

fn replace_all_checked(
    path: &Path,
    original: &str,
    replacement: &str,
    expected: usize,
    contract_error: &str,
) -> Result<(), Box<dyn Error>> {
    let contents = fs::read_to_string(path)?;
    let occurrences = contents.matches(original).count();
    if occurrences != expected {
        return Err(format!("{contract_error}: expected {expected}, found {occurrences}").into());
    }
    fs::remove_file(path)?;
    fs::write(path, contents.replace(original, replacement))?;
    Ok(())
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

fn emit_static_linking(
    steam_audio: &SteamAudioLibraries,
    libraries: &NativeLibraries,
) -> Result<(), Box<dyn Error>> {
    let mut steam_audio_link_order = vec![&steam_audio.phonon];
    steam_audio_link_order.extend(steam_audio.ispc_kernels.iter());
    steam_audio_link_order.extend([
        &libraries.pffft_library,
        &libraries.mysofa_library,
        &libraries.zlib_library,
    ]);
    for library in steam_audio_link_order {
        let directory = library
            .parent()
            .ok_or_else(|| format!("{} has no parent directory", library.display()))?;
        let name = static_link_name(library)?;
        println!("cargo:rustc-link-search=native={}", directory.display());
        println!("cargo:rustc-link-lib=static={name}");
    }
    if let Some(embree) = &libraries.embree {
        let mut embree_link_order = Vec::new();
        embree_link_order.push(&embree.sse2);
        embree_link_order.extend(embree.sse4.iter());
        embree_link_order.extend(embree.avx.iter());
        embree_link_order.extend(embree.avx2.iter());
        embree_link_order.extend([
            &embree.tasking,
            &embree.sys,
            &embree.simd,
            &embree.math,
            &embree.lexers,
        ]);
        for library in embree_link_order {
            let directory = library
                .parent()
                .ok_or_else(|| format!("{} has no parent directory", library.display()))?;
            let name = static_link_name(library)?;
            println!("cargo:rustc-link-search=native={}", directory.display());
            println!("cargo:rustc-link-lib=static={name}");
        }
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
