use super::*;

pub(super) const VERSION: &str = "1.5.2";

pub(super) fn build(
    workspace_root: &Path,
    native_root: &Path,
    configuration: &Configuration,
) -> anyhow::Result<()> {
    let source = workspace_root.join("vendor/opus");
    require_file(&source.join("CMakeLists.txt"), "Opus")?;
    let destination = blackflower_build::vendor_directory(native_root, configuration, "opus");
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
        .define("CMAKE_DISABLE_FIND_PACKAGE_Git", "ON")
        .define("OPUS_PACKAGE_VERSION", VERSION)
        .define("OPUS_BUILD_SHARED_LIBRARY", "OFF")
        .define("OPUS_BUILD_TESTING", "OFF")
        .define("OPUS_BUILD_PROGRAMS", "OFF")
        .define("OPUS_CUSTOM_MODES", "OFF")
        .define("OPUS_FIXED_POINT", "OFF")
        .define("OPUS_ENABLE_FLOAT_API", "ON")
        .define("OPUS_HARDENING", "ON")
        .define("OPUS_NONTHREADSAFE_PSEUDOSTACK", "OFF")
        .define("OPUS_DRED", "OFF")
        .define("OPUS_OSCE", "OFF")
        .define("OPUS_INSTALL_PKG_CONFIG_MODULE", "OFF")
        .define("OPUS_INSTALL_CMAKE_CONFIG_MODULE", "OFF")
        .define(
            "OPUS_STATIC_RUNTIME",
            if configuration.crt_static {
                "ON"
            } else {
                "OFF"
            },
        );
    let installed = config.build();
    write_vendor_manifest(&installed, configuration, Vendor::Opus, &source)
}
