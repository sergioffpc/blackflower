use super::*;

pub(super) const VERSION: &str = "1.3.3";

pub(super) fn build(
    workspace_root: &Path,
    native_root: &Path,
    configuration: &Configuration,
) -> anyhow::Result<()> {
    let vendor_source = workspace_root.join("vendor/libmysofa");
    require_file(&vendor_source.join("CMakeLists.txt"), "libmysofa")?;
    let destination = native_vendors::vendor_directory(native_root, configuration, "mysofa");
    let revision = source_revision(&vendor_source)?;
    let source = stage_source(
        &vendor_source,
        &destination.join("source"),
        &destination.join(native_vendors::MANIFEST_FILE),
        &revision,
    )?;
    patch_mysofa_zlib_discovery(&source)?;
    let zlib = native_vendors::vendor_directory(native_root, configuration, "zlib");
    let (architecture, operating_system) = target_platform(&configuration.target)?;
    let mut config = base_config(
        &source,
        &destination,
        configuration,
        architecture,
        operating_system,
    );
    config
        .build_target("install")
        .define("BUILD_TESTS", "OFF")
        .define("BUILD_SHARED_LIBS", "OFF")
        .define("BUILD_STATIC_LIBS", "ON")
        .define("ZLIB_ROOT", &zlib)
        .define("ZLIB_USE_STATIC_LIBS", "ON");
    let installed = config.build();
    native_vendors::write_manifest(
        &installed,
        configuration,
        Vendor::Mysofa.name(),
        Vendor::Mysofa.version(),
        &revision,
    )
    .map_err(|error| anyhow::anyhow!(error.to_string()))
}

fn patch_mysofa_zlib_discovery(source: &Path) -> anyhow::Result<()> {
    const REPLACEMENT: &str = "if(NOT WIN32)\n  find_library(MATH m)\nelse()\n  set(MATH \"\")\nendif()\nfind_package(ZLIB REQUIRED)\ninclude_directories(${ZLIB_INCLUDE_DIRS})\nset(PKG_CONFIG_PRIVATELIBS \"-lm -lz ${PKG_CONFIG_PRIVATELIBS}\")\n\n";
    let cmake_path = source.join("src/CMakeLists.txt");
    let contents = fs::read_to_string(&cmake_path)?;
    if contents.contains(REPLACEMENT) {
        return Ok(());
    }
    let start = contents
        .find("if(NOT MSVC)")
        .context("libmysofa CMake zlib discovery start was not found")?;
    let end = contents[start..]
        .find("set(libsrc")
        .map(|offset| start + offset)
        .context("libmysofa CMake zlib discovery end was not found")?;
    let mut patched = contents;
    patched.replace_range(start..end, REPLACEMENT);
    fs::write(cmake_path, patched)?;
    Ok(())
}
