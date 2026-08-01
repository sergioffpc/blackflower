#[allow(
    dead_code,
    reason = "the shared module exposes both producer and consumer halves of the native contract"
)]
#[path = "../../../build-support/native_vendors.rs"]
mod native_vendors;

use std::env;
use std::ffi::OsStr;
use std::fs;
use std::path::{Path, PathBuf};
use std::process::Command;

use anyhow::{Context, bail};
use clap::{Parser, Subcommand, ValueEnum};
use native_vendors::{CargoProfile, Configuration};

const EMBREE_VERSION: &str = "4.4.1";
const ISPC_VERSION: &str = "1.31.0";
const ZLIB_VERSION: &str = "1.3.1";

fn main() -> anyhow::Result<()> {
    run()
}

fn run() -> anyhow::Result<()> {
    let arguments = Arguments::parse();
    match arguments.command {
        Task::Build {
            profile,
            target,
            crt_static,
            mut vendors,
        } => {
            let workspace_root =
                fs::canonicalize(&arguments.workspace_root).with_context(|| {
                    format!(
                        "failed to resolve workspace root {}",
                        arguments.workspace_root.display()
                    )
                })?;
            if vendors.is_empty() {
                vendors = vec![Vendor::Embree, Vendor::Zlib];
            }
            build_vendors(&workspace_root, profile, target, crt_static, &vendors)
        }
    }
}

#[derive(Debug, Parser)]
#[command(name = "blackflower-native-build")]
struct Arguments {
    #[arg(long, default_value = ".")]
    workspace_root: PathBuf,
    #[command(subcommand)]
    command: Task,
}

#[derive(Debug, Subcommand)]
enum Task {
    /// Builds shared static libraries from the repository-level vendor directory.
    Build {
        #[arg(long, value_enum, default_value_t = BuildProfile::Debug)]
        profile: BuildProfile,
        #[arg(long)]
        target: Option<String>,
        #[arg(long)]
        crt_static: bool,
        #[arg(value_enum)]
        vendors: Vec<Vendor>,
    },
}

#[derive(Clone, Copy, Debug, ValueEnum)]
enum BuildProfile {
    Debug,
    Release,
}

impl From<BuildProfile> for CargoProfile {
    fn from(value: BuildProfile) -> Self {
        match value {
            BuildProfile::Debug => Self::Debug,
            BuildProfile::Release => Self::Release,
        }
    }
}

#[derive(Clone, Copy, Debug, ValueEnum)]
enum Vendor {
    Embree,
    Zlib,
}

impl Vendor {
    const fn name(self) -> &'static str {
        match self {
            Self::Embree => "embree",
            Self::Zlib => "zlib",
        }
    }
}

fn build_vendors(
    workspace_root: &Path,
    profile: BuildProfile,
    target: Option<String>,
    crt_static: bool,
    vendors: &[Vendor],
) -> anyhow::Result<()> {
    let host = rustc_host()?;
    let target = target.unwrap_or_else(|| host.clone());
    if target != host {
        bail!(
            "shared native vendor prebuilds currently require a native target; host is {host}, requested {target}"
        );
    }
    let configuration = Configuration::new(target, profile.into(), crt_static);
    let native_root = native_vendors::native_root(workspace_root);
    for vendor in vendors {
        match vendor {
            Vendor::Embree => build_embree(workspace_root, &native_root, &configuration)?,
            Vendor::Zlib => build_zlib(workspace_root, &native_root, &configuration)?,
        }
        println!(
            "prepared {} at {}",
            vendor.name(),
            native_vendors::vendor_directory(&native_root, &configuration, vendor.name()).display()
        );
    }
    Ok(())
}

fn rustc_host() -> anyhow::Result<String> {
    let rustc = env::var_os("RUSTC").unwrap_or_else(|| OsStr::new("rustc").to_os_string());
    let output = Command::new(rustc)
        .arg("-vV")
        .output()
        .context("failed to execute rustc -vV")?;
    if !output.status.success() {
        bail!("rustc -vV failed with {}", output.status);
    }
    let stdout = String::from_utf8(output.stdout).context("rustc -vV did not emit UTF-8")?;
    stdout
        .lines()
        .find_map(|line| line.strip_prefix("host: "))
        .map(str::to_owned)
        .context("rustc -vV did not report a host triple")
}

#[allow(
    clippy::too_many_lines,
    reason = "the pinned Embree configuration is kept together as one auditable build contract"
)]
fn build_embree(
    workspace_root: &Path,
    native_root: &Path,
    configuration: &Configuration,
) -> anyhow::Result<()> {
    let source = workspace_root.join("vendor/embree");
    require_file(&source.join("CMakeLists.txt"), "Embree")?;
    verify_embree_version(&source)?;
    let (architecture, operating_system) = target_platform(&configuration.target)?;
    if !matches!(architecture, "x86_64" | "aarch64")
        || !matches!(operating_system, "linux" | "macos" | "windows")
    {
        bail!(
            "Embree prebuild does not support target {}",
            configuration.target
        );
    }

    let ispc = (architecture == "x86_64")
        .then(|| find_ispc(operating_system))
        .transpose()?;
    let destination = native_vendors::vendor_directory(native_root, configuration, "embree");
    let mut config = base_config(
        &source,
        &destination,
        configuration,
        architecture,
        operating_system,
    );
    config
        .build_target("install")
        .define("BUILD_TESTING", "OFF")
        .define("EMBREE_STATIC_LIB", "ON")
        .define(
            "EMBREE_STATIC_RUNTIME",
            if configuration.crt_static {
                "ON"
            } else {
                "OFF"
            },
        )
        .define(
            "EMBREE_ISPC_SUPPORT",
            if ispc.is_some() { "ON" } else { "OFF" },
        )
        .define("EMBREE_SYCL_SUPPORT", "OFF")
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
    if let Some(executable) = &ispc {
        config.define("EMBREE_ISPC_EXECUTABLE", executable);
    }
    if architecture == "aarch64" {
        config.define("EMBREE_MAX_ISA", "NONE");
    } else if operating_system == "macos" {
        config
            .define("EMBREE_ISA_SSE2", "ON")
            .define("EMBREE_ISA_SSE42", "OFF")
            .define("EMBREE_ISA_AVX", "OFF")
            .define("EMBREE_ISA_AVX2", "OFF")
            .define("EMBREE_ISA_AVX512", "OFF");
    } else {
        config.define("EMBREE_MAX_ISA", "AVX2");
    }
    let installed = config.build();
    require_file(
        &installed.join("include/embree4/rtcore.h"),
        "Embree install",
    )?;
    native_vendors::write_manifest(
        &installed,
        configuration,
        "embree",
        EMBREE_VERSION,
        &source_revision(&source)?,
    )
    .map_err(|error| anyhow::anyhow!(error.to_string()))?;
    Ok(())
}

fn build_zlib(
    workspace_root: &Path,
    native_root: &Path,
    configuration: &Configuration,
) -> anyhow::Result<()> {
    let vendor_source = workspace_root.join("vendor/zlib");
    require_file(&vendor_source.join("CMakeLists.txt"), "zlib")?;
    verify_zlib_version(&vendor_source)?;
    let (architecture, operating_system) = target_platform(&configuration.target)?;
    let destination = native_vendors::vendor_directory(native_root, configuration, "zlib");
    let revision = source_revision(&vendor_source)?;
    let source = stage_source(
        &vendor_source,
        &destination.join("source"),
        &destination.join(native_vendors::MANIFEST_FILE),
        &revision,
    )?;
    let mut config = base_config(
        &source,
        &destination,
        configuration,
        architecture,
        operating_system,
    );
    config
        .build_target("zlibstatic")
        .define("BUILD_SHARED_LIBS", "OFF")
        .define("ZLIB_BUILD_EXAMPLES", "OFF");
    let built = config.build();
    let library = find_static_library(
        &built.join("build"),
        operating_system == "windows",
        "z",
        "zlibstatic",
    )?;
    let generated_header = built.join("build/zconf.h");
    require_file(&generated_header, "zlib generated header")?;
    let include = built.join("include");
    let libraries = built.join("lib");
    fs::create_dir_all(&include)?;
    fs::create_dir_all(&libraries)?;
    fs::copy(source.join("zlib.h"), include.join("zlib.h"))?;
    fs::copy(generated_header, include.join("zconf.h"))?;
    let library_name = library
        .file_name()
        .context("zlib static library has no file name")?;
    fs::copy(&library, libraries.join(library_name))?;
    native_vendors::write_manifest(&built, configuration, "zlib", ZLIB_VERSION, &revision)
        .map_err(|error| anyhow::anyhow!(error.to_string()))?;
    Ok(())
}

fn stage_source(
    source: &Path,
    destination: &Path,
    manifest: &Path,
    revision: &str,
) -> anyhow::Result<PathBuf> {
    if destination.join("CMakeLists.txt").is_file()
        && fs::read_to_string(manifest).is_ok_and(|contents| {
            contents
                .lines()
                .any(|line| line == format!("source_revision={revision}"))
        })
    {
        return Ok(destination.to_path_buf());
    }
    if destination.exists() {
        fs::remove_dir_all(destination)?;
    }
    copy_tree(source, destination)?;
    Ok(destination.to_path_buf())
}

fn copy_tree(source: &Path, destination: &Path) -> anyhow::Result<()> {
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
        } else {
            fs::copy(source_path, destination_path)?;
        }
    }
    Ok(())
}

fn base_config(
    source: &Path,
    output: &Path,
    configuration: &Configuration,
    architecture: &str,
    operating_system: &str,
) -> cmake::Config {
    let mut config = cmake::Config::new(source);
    config
        .out_dir(output)
        .target(&configuration.target)
        .host(&configuration.target)
        .profile(configuration.cmake_profile)
        .pic(true)
        .static_crt(configuration.crt_static)
        .define("CMAKE_POLICY_VERSION_MINIMUM", "3.5")
        .define("CMAKE_POSITION_INDEPENDENT_CODE", "ON");
    if operating_system == "macos" {
        let (cmake_architecture, deployment_target) = match architecture {
            "aarch64" => ("arm64", "11.0"),
            _ => (architecture, "10.13"),
        };
        config
            .define("CMAKE_OSX_ARCHITECTURES", cmake_architecture)
            .define("CMAKE_OSX_DEPLOYMENT_TARGET", deployment_target);
    }
    if configuration.target.ends_with("-msvc") {
        config.cxxflag("/EHsc").define(
            "CMAKE_MSVC_RUNTIME_LIBRARY",
            if configuration.crt_static {
                "MultiThreaded$<$<CONFIG:Debug>:Debug>"
            } else {
                "MultiThreaded$<$<CONFIG:Debug>:Debug>DLL"
            },
        );
    }
    config
}

fn target_platform(target: &str) -> anyhow::Result<(&str, &str)> {
    let architecture = target
        .split('-')
        .next()
        .context("target triple has no architecture")?;
    let operating_system = if target.contains("windows") {
        "windows"
    } else if target.contains("apple-darwin") {
        "macos"
    } else if target.contains("linux") {
        "linux"
    } else {
        bail!("unsupported native vendor target {target}");
    };
    Ok((architecture, operating_system))
}

fn verify_embree_version(source: &Path) -> anyhow::Result<()> {
    let build = fs::read_to_string(source.join("CMakeLists.txt"))?;
    let component = |name: &str| {
        build.lines().find_map(|line| {
            line.trim()
                .strip_prefix(&format!("SET(EMBREE_VERSION_{name} "))
                .and_then(|value| value.strip_suffix(')'))
        })
    };
    let version = format!(
        "{}.{}.{}",
        component("MAJOR").context("Embree has no major version")?,
        component("MINOR").context("Embree has no minor version")?,
        component("PATCH").context("Embree has no patch version")?
    );
    if version != EMBREE_VERSION {
        bail!("Embree source version is {version}; expected {EMBREE_VERSION}");
    }
    Ok(())
}

fn verify_zlib_version(source: &Path) -> anyhow::Result<()> {
    let build = fs::read_to_string(source.join("CMakeLists.txt"))?;
    let version = build
        .lines()
        .find_map(|line| {
            line.trim()
                .strip_prefix("set(VERSION \"")
                .and_then(|value| value.strip_suffix("\")"))
        })
        .context("zlib does not declare its version")?;
    if version != ZLIB_VERSION {
        bail!("zlib source version is {version}; expected {ZLIB_VERSION}");
    }
    Ok(())
}

fn find_ispc(operating_system: &str) -> anyhow::Result<PathBuf> {
    let executable = env::var_os("BLACKFLOWER_ISPC")
        .map(PathBuf::from)
        .or_else(|| {
            env::var_os("PATH").and_then(|path| {
                env::split_paths(&path)
                    .map(|directory| {
                        directory.join(if operating_system == "windows" {
                            "ispc.exe"
                        } else {
                            "ispc"
                        })
                    })
                    .find(|candidate| candidate.is_file())
            })
        })
        .context("Embree x86-64 prebuild requires ISPC 1.31.0; set BLACKFLOWER_ISPC")?;
    let output = Command::new(&executable).arg("--version").output()?;
    let version = String::from_utf8_lossy(&output.stdout);
    if !output.status.success() || !version.contains(ISPC_VERSION) {
        bail!(
            "{} is not ISPC {ISPC_VERSION}: {}",
            executable.display(),
            version.trim()
        );
    }
    Ok(executable)
}

fn require_file(path: &Path, description: &str) -> anyhow::Result<()> {
    if path.is_file() {
        Ok(())
    } else {
        bail!(
            "missing {description} file {}; initialize global native vendors with `git submodule update --init --recursive vendor/embree vendor/zlib`",
            path.display()
        )
    }
}

fn find_static_library(
    root: &Path,
    windows: bool,
    unix_name: &str,
    windows_name: &str,
) -> anyhow::Result<PathBuf> {
    if windows {
        let release = format!("{windows_name}.lib");
        find_built_file(root, &release).or_else(|_error| {
            let debug = format!("{windows_name}d.lib");
            find_built_file(root, &debug)
        })
    } else {
        find_built_file(root, &format!("lib{unix_name}.a"))
    }
}

fn find_built_file(root: &Path, file_name: &str) -> anyhow::Result<PathBuf> {
    if !root.is_dir() {
        bail!("native build directory {} does not exist", root.display());
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
    bail!(
        "native build did not produce {file_name} below {}",
        root.display()
    )
}

fn source_revision(source: &Path) -> anyhow::Result<String> {
    let output = Command::new("git")
        .args(["rev-parse", "HEAD"])
        .current_dir(source)
        .output()
        .with_context(|| format!("failed to inspect source revision in {}", source.display()))?;
    if !output.status.success() {
        bail!("git rev-parse failed in {}", source.display());
    }
    String::from_utf8(output.stdout)
        .context("git rev-parse emitted non-UTF-8 output")
        .map(|revision| revision.trim().to_owned())
}
