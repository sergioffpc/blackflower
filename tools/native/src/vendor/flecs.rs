use super::*;

pub(super) const VERSION: &str = "4.1.6";

pub(super) fn build(
    workspace_root: &Path,
    native_root: &Path,
    configuration: &Configuration,
) -> anyhow::Result<()> {
    let source = workspace_root.join("vendor/flecs");
    require_file(&source.join("distr/flecs.c"), "Flecs amalgamation")?;
    let project = workspace_root.join("tools/native/cmake/flecs");
    let config_dir = workspace_root.join("crates/blackflower-ecs/native");
    let destination = blackflower_build::vendor_directory(native_root, configuration, "flecs");
    let (architecture, operating_system) = target_platform(&configuration.target)?;
    let mut config = base_config(
        &project,
        &destination,
        configuration,
        architecture,
        operating_system,
    );
    config
        .build_target("install")
        .define_path("BLACKFLOWER_FLECS_ROOT", &source)
        .define_path("BLACKFLOWER_FLECS_CONFIG_DIR", config_dir);
    let installed = config.build();
    write_vendor_manifest(&installed, configuration, Vendor::Flecs, &source)
}
