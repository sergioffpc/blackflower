use super::*;

pub(super) fn native_parallelism() -> Option<String> {
    env::var("CMAKE_BUILD_PARALLEL_LEVEL")
        .ok()
        .filter(|value| value.parse::<usize>().is_ok_and(|jobs| jobs > 0))
        .or_else(|| {
            env::var("NUM_JOBS")
                .ok()
                .filter(|value| value.parse::<usize>().is_ok_and(|jobs| jobs > 0))
        })
        .or_else(|| {
            env::var("CARGO_BUILD_JOBS")
                .ok()
                .filter(|value| value.parse::<usize>().is_ok_and(|jobs| jobs > 0))
        })
        .or_else(|| {
            std::thread::available_parallelism()
                .ok()
                .map(|jobs| jobs.get().to_string())
        })
}

pub(super) fn write_vendor_manifest(
    installed: &Path,
    configuration: &Configuration,
    vendor: Vendor,
    source: &Path,
) -> anyhow::Result<()> {
    native_vendors::write_manifest(
        installed,
        configuration,
        vendor.name(),
        vendor.version(),
        &source_revision(source)?,
    )
    .map_err(|error| anyhow::anyhow!(error.to_string()))
}

pub(super) fn stage_source(
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

pub(super) fn copy_tree(source: &Path, destination: &Path) -> anyhow::Result<()> {
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

pub(super) fn base_config(
    source: &Path,
    output: &Path,
    configuration: &Configuration,
    architecture: &str,
    operating_system: &str,
) -> cmake::Config {
    let mut config = cmake::Config::new(source);
    if let Some(jobs) = native_parallelism() {
        config.env("CMAKE_BUILD_PARALLEL_LEVEL", jobs);
    }
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

pub(super) fn target_platform(target: &str) -> anyhow::Result<(&str, &str)> {
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

pub(super) fn require_file(path: &Path, description: &str) -> anyhow::Result<()> {
    if path.is_file() {
        Ok(())
    } else {
        bail!(
            "missing {description} file {}; initialize global native vendors with `git submodule update --init --recursive`",
            path.display()
        )
    }
}

pub(super) fn find_static_library(
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

pub(super) fn find_built_file(root: &Path, file_name: &str) -> anyhow::Result<PathBuf> {
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

pub(super) fn source_revision(source: &Path) -> anyhow::Result<String> {
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
