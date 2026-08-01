use super::*;

pub(super) const VERSION: &str = "2026.14.1";

pub(super) fn build(
    workspace_root: &Path,
    native_root: &Path,
    configuration: &Configuration,
) -> anyhow::Result<()> {
    let source = workspace_root.join("vendor/slang");
    require_file(&source.join("CMakeLists.txt"), "Slang")?;
    let project = workspace_root.join("tools/native/cmake/slang");
    let destination = native_vendors::vendor_directory(native_root, configuration, "slang");
    let (architecture, operating_system) = target_platform(&configuration.target)?;
    let mut config = base_config(
        &project,
        &destination,
        configuration,
        architecture,
        operating_system,
    );
    config
        .build_target("blackflower_slang_install")
        .define("BLACKFLOWER_SLANG_ROOT", &source);
    let installed = config.build();
    write_vendor_manifest(&installed, configuration, Vendor::Slang, &source)
}
