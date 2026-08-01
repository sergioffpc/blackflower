use super::*;

pub(super) const VERSION: &str = "1.3.1";

pub(super) fn build(
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
    native_vendors::write_manifest(&built, configuration, "zlib", VERSION, &revision)
        .map_err(|error| anyhow::anyhow!(error.to_string()))?;
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
    if version != VERSION {
        bail!("zlib source version is {version}; expected {VERSION}");
    }
    Ok(())
}
