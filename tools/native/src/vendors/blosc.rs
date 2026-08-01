use super::*;

pub(super) const VERSION: &str = "1.21.6";

pub(super) fn build(
    workspace_root: &Path,
    native_root: &Path,
    configuration: &Configuration,
) -> anyhow::Result<()> {
    let source = workspace_root.join("vendor/c-blosc");
    require_file(&source.join("CMakeLists.txt"), "c-blosc")?;
    let destination = native_vendors::vendor_directory(native_root, configuration, "blosc");
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
        .define("BUILD_SHARED", "OFF")
        .define("BUILD_STATIC", "ON")
        .define("BUILD_TESTS", "OFF")
        .define("BUILD_FUZZERS", "OFF")
        .define("BUILD_BENCHMARKS", "OFF")
        .define("DEACTIVATE_SNAPPY", "ON")
        .define("DEACTIVATE_ZLIB", "ON")
        .define("DEACTIVATE_ZSTD", "ON")
        .define("BLOSC_INSTALL", "ON");
    let installed = config.build();
    write_vendor_manifest(&installed, configuration, Vendor::Blosc, &source)
}
