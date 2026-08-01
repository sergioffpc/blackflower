use super::*;

pub(super) const VERSION: &str = "2022.1.0";

pub(super) fn build(
    workspace_root: &Path,
    native_root: &Path,
    configuration: &Configuration,
) -> anyhow::Result<()> {
    let source = workspace_root.join("vendor/oneTBB");
    require_file(&source.join("CMakeLists.txt"), "oneTBB")?;
    let destination = blackflower_build::vendor_directory(native_root, configuration, "tbb");
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
        .define("BUILD_SHARED_LIBS", "OFF")
        .define("TBB_TEST", "OFF")
        .define("TBBMALLOC_BUILD", "OFF")
        .define("TBBMALLOC_PROXY_BUILD", "OFF")
        .define("TBB_EXAMPLES", "OFF")
        .define("TBB_STRICT", "OFF");
    let installed = config.build();
    write_vendor_manifest(&installed, configuration, Vendor::Tbb, &source)
}
