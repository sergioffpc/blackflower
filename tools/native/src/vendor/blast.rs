use super::*;

pub(super) const VERSION: &str = "5.0.6";

pub(super) fn build(
    workspace_root: &Path,
    native_root: &Path,
    configuration: &Configuration,
) -> anyhow::Result<()> {
    let source = workspace_root.join("vendor/PhysX");
    require_file(
        &source.join("blast/include/lowlevel/NvBlast.h"),
        "NVIDIA Blast",
    )?;
    let project = workspace_root.join("tools/native/cmake/blast");
    let destination = blackflower_build::vendor_directory(native_root, configuration, "blast");
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
    write_vendor_manifest(&installed, configuration, Vendor::Blast, &source)
}
