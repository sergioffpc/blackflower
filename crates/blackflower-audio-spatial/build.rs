use std::env;
use std::error::Error;
use std::fs;
use std::panic::{AssertUnwindSafe, catch_unwind};
use std::path::{Path, PathBuf};

const EMBREE_VERSION: &str = "4.4.1";
const MYSOFA_VERSION: &str = "1.3.3";
const PFFFT_VERSION: &str = "e0bf595c98ded55cc457a371c1b29c8cab552628";
const STEAM_AUDIO_VERSION: &str = "4.8.1";
const ZLIB_VERSION: &str = "1.3.1";
const WRAPPER_HEADER: &str = "native/wrapper.h";

struct EmbreeLibraries {
    sse2: PathBuf,
    sse4: Option<PathBuf>,
    avx: Option<PathBuf>,
    avx2: Option<PathBuf>,
    tasking: PathBuf,
    sys: PathBuf,
    simd: PathBuf,
    math: PathBuf,
    lexers: PathBuf,
}

fn main() -> Result<(), Box<dyn Error>> {
    println!("cargo:rustc-check-cfg=cfg(blackflower_steam_audio_embree)");
    println!("cargo:rustc-cfg=blackflower_steam_audio_embree");
    println!("cargo:rerun-if-changed={WRAPPER_HEADER}");
    blackflower_build::emit_cargo_directives();

    let manifest_dir =
        PathBuf::from(env::var_os("CARGO_MANIFEST_DIR").ok_or("CARGO_MANIFEST_DIR is not set")?);
    let (configuration, workspace_root, steam_audio) =
        blackflower_build::locate_from_cargo_build_script(
            &manifest_dir,
            "steam-audio",
            STEAM_AUDIO_VERSION,
        )
        .map_err(blackflower_build_error)?;
    let steam_audio_source = workspace_root.join("vendor/steam-audio-sdk/core");
    let version_template = steam_audio_source.join("src/core/phonon_version.h.in");
    let include = steam_audio_source.join("src/core");
    require_path(&version_template)?;
    require_path(&include.join("phonon.h"))?;

    let out_dir = PathBuf::from(env::var_os("OUT_DIR").ok_or("OUT_DIR is not set")?);
    generate_version_header(&version_template, &out_dir)?;
    generate_bindings(&out_dir, &include)?;
    link_vendor_libraries(&manifest_dir, &steam_audio, &configuration)
}

fn link_vendor_libraries(
    manifest_dir: &Path,
    steam_audio: &Path,
    configuration: &blackflower_build::Configuration,
) -> Result<(), Box<dyn Error>> {
    let phonon =
        blackflower_build::find_static_library(steam_audio, configuration, "phonon", "phonon")
            .map_err(blackflower_build_error)?;
    let ispc_kernels = (env::var("CARGO_CFG_TARGET_ARCH")? == "x86_64")
        .then(|| {
            blackflower_build::find_static_library(
                steam_audio,
                configuration,
                "ispckernels",
                "ispckernels",
            )
            .map_err(blackflower_build_error)
        })
        .transpose()?;
    let pffft = locate(manifest_dir, "pffft", PFFFT_VERSION)?;
    let pffft_library =
        blackflower_build::find_static_library(&pffft, configuration, "pffft", "pffft")
            .map_err(blackflower_build_error)?;
    let mysofa = locate(manifest_dir, "mysofa", MYSOFA_VERSION)?;
    let mysofa_library =
        blackflower_build::find_static_library(&mysofa, configuration, "mysofa", "mysofa")
            .map_err(blackflower_build_error)?;
    let zlib = locate(manifest_dir, "zlib", ZLIB_VERSION)?;
    let zlib_library =
        blackflower_build::find_static_library(&zlib, configuration, "z", "zlibstatic")
            .map_err(blackflower_build_error)?;
    let embree = locate(manifest_dir, "embree", EMBREE_VERSION)?;
    let embree_libraries = load_embree(&embree, configuration)?;

    emit_libraries([
        Some(phonon.as_path()),
        ispc_kernels.as_deref(),
        Some(pffft_library.as_path()),
        Some(mysofa_library.as_path()),
        Some(zlib_library.as_path()),
        Some(embree_libraries.sse2.as_path()),
        embree_libraries.sse4.as_deref(),
        embree_libraries.avx.as_deref(),
        embree_libraries.avx2.as_deref(),
        Some(embree_libraries.tasking.as_path()),
        Some(embree_libraries.sys.as_path()),
        Some(embree_libraries.simd.as_path()),
        Some(embree_libraries.math.as_path()),
        Some(embree_libraries.lexers.as_path()),
    ])?;
    link_platform_libraries()?;
    Ok(())
}

fn emit_libraries<const N: usize>(libraries: [Option<&Path>; N]) -> Result<(), Box<dyn Error>> {
    for library in libraries.into_iter().flatten() {
        blackflower_build::emit_static_library(library).map_err(blackflower_build_error)?;
    }
    Ok(())
}

fn locate(manifest_dir: &Path, name: &str, version: &str) -> Result<PathBuf, Box<dyn Error>> {
    blackflower_build::locate_from_cargo_build_script(manifest_dir, name, version)
        .map(|(_configuration, _workspace_root, directory)| directory)
        .map_err(|error| blackflower_build_error(error).into())
}

fn load_embree(
    root: &Path,
    configuration: &blackflower_build::Configuration,
) -> Result<EmbreeLibraries, Box<dyn Error>> {
    let architecture = env::var("CARGO_CFG_TARGET_ARCH")?;
    let operating_system = env::var("CARGO_CFG_TARGET_OS")?;
    let has_x86_variants = architecture == "x86_64" && operating_system != "macos";
    let has_avx2 = has_x86_variants || (architecture == "aarch64" && operating_system == "macos");
    let find = |unix: &str, windows: &str| {
        blackflower_build::find_static_library(root, configuration, unix, windows)
            .map_err(blackflower_build_error)
    };
    Ok(EmbreeLibraries {
        sse2: find("embree", "embree")?,
        sse4: has_x86_variants
            .then(|| find("embree_sse42", "embree_sse42"))
            .transpose()?,
        avx: has_x86_variants
            .then(|| find("embree_avx", "embree_avx"))
            .transpose()?,
        avx2: has_avx2
            .then(|| find("embree_avx2", "embree_avx2"))
            .transpose()?,
        tasking: find("tasking", "tasking")?,
        sys: find("sys", "sys")?,
        simd: find("simd", "simd")?,
        math: find("math", "math")?,
        lexers: find("lexers", "lexers")?,
    })
}

fn generate_version_header(template: &Path, out_dir: &Path) -> Result<(), Box<dyn Error>> {
    let header = fs::read_to_string(template)?
        .replace("@PROJECT_VERSION_MAJOR@", "4")
        .replace("@PROJECT_VERSION_MINOR@", "8")
        .replace("@PROJECT_VERSION_PATCH@", "1");
    fs::write(out_dir.join("phonon_version.h"), header)?;
    Ok(())
}

fn generate_bindings(out_dir: &Path, include: &Path) -> Result<(), Box<dyn Error>> {
    let target = env::var("TARGET")?;
    let builder = bindgen::Builder::default()
        .header(WRAPPER_HEADER)
        .clang_arg(format!("-I{}", out_dir.display()))
        .clang_arg(format!("-I{}", include.display()))
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
        "failed to load libclang for Steam Audio bindings; install libclang and set LIBCLANG_PATH"
    })?;
    generation
        .map_err(|error| format!("failed to generate Steam Audio bindings: {error}"))?
        .write_to_file(out_dir.join("steam_audio_bindings.rs"))?;
    Ok(())
}

fn require_path(path: &Path) -> Result<(), Box<dyn Error>> {
    if path.exists() {
        Ok(())
    } else {
        Err(format!("missing {}", path.display()).into())
    }
}

fn link_platform_libraries() -> Result<(), Box<dyn Error>> {
    match env::var("CARGO_CFG_TARGET_OS")?.as_str() {
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

fn blackflower_build_error(error: Box<dyn Error + Send + Sync>) -> std::io::Error {
    std::io::Error::other(error.to_string())
}
