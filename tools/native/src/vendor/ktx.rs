use super::*;

pub(super) const VERSION: &str = "4.4.2";

pub(super) fn build(
    workspace_root: &Path,
    native_root: &Path,
    configuration: &Configuration,
) -> anyhow::Result<()> {
    let source = workspace_root.join("vendor/KTX-Software");
    require_file(&source.join("CMakeLists.txt"), "KTX-Software")?;
    let project = workspace_root.join("tools/native/cmake/ktx");
    let destination = blackflower_build::vendor_directory(native_root, configuration, "ktx");
    let (architecture, operating_system) = target_platform(&configuration.target)?;
    let mut config = base_config(
        &project,
        &destination,
        configuration,
        architecture,
        operating_system,
    );
    config
        .build_target("blackflower_ktx_install")
        .define_path("BLACKFLOWER_KTX_ROOT", &source);
    let installed = config.build();
    write_vendor_manifest(&installed, configuration, Vendor::Ktx, &source)
}
