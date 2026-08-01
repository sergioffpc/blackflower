use super::*;

pub(super) const VERSION: &str = "0.16.0";

pub(super) fn build(
    workspace_root: &Path,
    native_root: &Path,
    configuration: &Configuration,
) -> anyhow::Result<()> {
    let source = workspace_root.join("vendor/ozz-animation");
    require_file(&source.join("CMakeLists.txt"), "ozz-animation")?;
    let destination = native_vendors::vendor_directory(native_root, configuration, "ozz");
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
        .define("ozz_build_data", "OFF")
        .define("ozz_build_fbx", "OFF")
        .define("ozz_build_gltf", "ON")
        .define("ozz_build_howtos", "OFF")
        .define(
            "ozz_build_msvc_rt_dll",
            if configuration.crt_static {
                "OFF"
            } else {
                "ON"
            },
        )
        .define("ozz_build_postfix", "OFF")
        .define("ozz_build_samples", "OFF")
        .define("ozz_build_tests", "OFF")
        .define("ozz_build_tools", "ON");
    let installed = config.build();
    write_vendor_manifest(&installed, configuration, Vendor::Ozz, &source)
}
