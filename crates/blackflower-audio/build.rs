use std::env;
use std::error::Error;
use std::fs;
use std::panic::{AssertUnwindSafe, catch_unwind};
use std::path::{Path, PathBuf};

const EXPECTED_SDK_VERSION: &str = "4.8.1";
const SDK_BUILD: &str = "vendor/steam-audio-sdk/core/CMakeLists.txt";
const SDK_INCLUDE: &str = "vendor/steam-audio-sdk/core/src/core";
const SDK_HEADER: &str = "vendor/steam-audio-sdk/core/src/core/phonon.h";
const SDK_VERSION_TEMPLATE: &str = "vendor/steam-audio-sdk/core/src/core/phonon_version.h.in";
const WRAPPER_HEADER: &str = "native/wrapper.h";

fn main() -> Result<(), Box<dyn Error>> {
    for path in [SDK_BUILD, SDK_HEADER, SDK_VERSION_TEMPLATE, WRAPPER_HEADER] {
        println!("cargo:rerun-if-changed={path}");
        require_file(Path::new(path))?;
    }
    let out_dir = PathBuf::from(env::var_os("OUT_DIR").ok_or("OUT_DIR is not set")?);
    generate_version_header(&out_dir)?;
    generate_bindings(&out_dir)?;
    Ok(())
}

fn require_file(path: &Path) -> Result<(), Box<dyn Error>> {
    if path.is_file() {
        return Ok(());
    }

    Err(format!(
        "missing {}; initialize the Steam Audio submodule with \
         `git submodule update --init --recursive`",
        path.display()
    )
    .into())
}

fn generate_version_header(out_dir: &Path) -> Result<(), Box<dyn Error>> {
    let build = fs::read_to_string(SDK_BUILD)?;
    let version = build
        .lines()
        .find_map(|line| {
            line.trim()
                .strip_prefix("project(Phonon VERSION ")
                .and_then(|value| value.strip_suffix(')'))
        })
        .ok_or("Steam Audio does not declare its project version")?;
    if version != EXPECTED_SDK_VERSION {
        return Err(format!(
            "Steam Audio submodule version is {version}; expected {EXPECTED_SDK_VERSION}"
        )
        .into());
    }
    let mut components = version.split('.');
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
        .dynamic_library_name("SteamAudioApi")
        .dynamic_link_require_all(true)
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
