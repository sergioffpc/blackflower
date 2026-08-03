use super::*;

pub(super) const VERSION: &str = "1.12.0";

pub(super) fn build(
    workspace_root: &Path,
    native_root: &Path,
    configuration: &Configuration,
) -> anyhow::Result<()> {
    let source = workspace_root.join("vendor/flatbuffers");
    require_file(&source.join("CMakeLists.txt"), "FlatBuffers")?;
    let destination =
        blackflower_build::vendor_directory(native_root, configuration, "flatbuffers");
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
        .define("FLATBUFFERS_BUILD_TESTS", "OFF")
        .define("FLATBUFFERS_BUILD_FLATLIB", "OFF")
        .define("FLATBUFFERS_BUILD_FLATC", "ON")
        .define("FLATBUFFERS_BUILD_FLATHASH", "OFF")
        .define("FLATBUFFERS_BUILD_GRPCTEST", "OFF")
        .define("FLATBUFFERS_BUILD_SHAREDLIB", "OFF");
    let installed = config.build();
    write_vendor_manifest(&installed, configuration, Vendor::Flatbuffers, &source)
}
