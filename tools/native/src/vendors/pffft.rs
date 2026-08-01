use super::*;

pub(super) const VERSION: &str = "e0bf595c98ded55cc457a371c1b29c8cab552628";

pub(super) fn build(
    workspace_root: &Path,
    native_root: &Path,
    configuration: &Configuration,
) -> anyhow::Result<()> {
    let source = workspace_root.join("vendor/pffft");
    require_file(&source.join("CMakeLists.txt"), "PFFFT")?;
    let destination = native_vendors::vendor_directory(native_root, configuration, "pffft");
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
        .define("PFFFT_USE_TYPE_DOUBLE", "OFF")
        .define("PFFFT_USE_BENCH_GREEN", "OFF")
        .define("PFFFT_USE_BENCH_KISS", "OFF")
        .define("PFFFT_USE_BENCH_POCKET", "OFF")
        .define("PFFFT_USE_FFTPACK", "OFF");
    let installed = config.build();
    write_vendor_manifest(&installed, configuration, Vendor::Pffft, &source)
}
