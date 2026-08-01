use super::*;

pub(super) const VERSION: &str = "4.4.1";
pub(super) const ISPC_VERSION: &str = "1.31.0";

#[allow(
    clippy::too_many_lines,
    reason = "the pinned Embree configuration is kept together as one auditable build contract"
)]
pub(super) fn build(
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
    let destination = blackflower_build::vendor_directory(native_root, configuration, "embree");
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
    blackflower_build::write_manifest(
        &installed,
        configuration,
        "embree",
        VERSION,
        &source_revision(&source)?,
    )
    .map_err(|error| anyhow::anyhow!(error.to_string()))?;
    Ok(())
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
    if version != VERSION {
        bail!("Embree source version is {version}; expected {VERSION}");
    }
    Ok(())
}

pub(super) fn find_ispc(operating_system: &str) -> anyhow::Result<PathBuf> {
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
