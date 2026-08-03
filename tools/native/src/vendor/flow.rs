use super::*;

pub(super) const VERSION: &str = "2.2.0";

pub(super) fn build(
    workspace_root: &Path,
    native_root: &Path,
    configuration: &Configuration,
) -> anyhow::Result<()> {
    let source = workspace_root.join("vendor/PhysX");
    require_file(
        &source.join("flow/include/nvflow/NvFlowContext.h"),
        "NVIDIA Flow",
    )?;
    let project = workspace_root.join("tools/native/cmake/flow");
    let destination = blackflower_build::vendor_directory(native_root, configuration, "flow");
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
        .define_path("BLACKFLOWER_PHYSX_ROOT", &source);
    let installed = config.build();
    write_vendor_manifest(&installed, configuration, Vendor::Flow, &source)
}
