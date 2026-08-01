use super::*;

pub(super) const VERSION: &str = "0.731.0";

pub(super) fn build(
    workspace_root: &Path,
    native_root: &Path,
    configuration: &Configuration,
) -> anyhow::Result<()> {
    let source = workspace_root.join("vendor/luau");
    require_file(&source.join("CMakeLists.txt"), "Luau")?;
    let project = workspace_root.join("tools/native/cmake/luau");
    let destination = native_vendors::vendor_directory(native_root, configuration, "luau");
    let (architecture, operating_system) = target_platform(&configuration.target)?;
    let mut config = base_config(
        &project,
        &destination,
        configuration,
        architecture,
        operating_system,
    );
    config
        .build_target("blackflower_luau_install")
        .define("BLACKFLOWER_LUAU_ROOT", &source)
        .define("LUAU_WERROR", "OFF")
        .define(
            "LUAU_STATIC_CRT",
            if configuration.crt_static {
                "ON"
            } else {
                "OFF"
            },
        );
    let installed = config.build();
    write_vendor_manifest(&installed, configuration, Vendor::Luau, &source)
}
