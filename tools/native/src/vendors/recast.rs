use super::*;

pub(super) const VERSION: &str = "1.6.0";

pub(super) fn build(
    workspace_root: &Path,
    native_root: &Path,
    configuration: &Configuration,
) -> anyhow::Result<()> {
    let source = workspace_root.join("vendor/recastnavigation");
    require_file(&source.join("CMakeLists.txt"), "RecastNavigation")?;
    let destination = native_vendors::vendor_directory(native_root, configuration, "recast");
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
        .define("RECASTNAVIGATION_DEMO", "OFF")
        .define("RECASTNAVIGATION_EXAMPLES", "OFF")
        .define("RECASTNAVIGATION_TESTS", "OFF");
    let installed = config.build();
    write_vendor_manifest(&installed, configuration, Vendor::Recast, &source)
}
