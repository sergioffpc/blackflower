use super::*;

pub(super) const VERSION: &str = "13.0.0";

pub(super) fn build(
    workspace_root: &Path,
    native_root: &Path,
    configuration: &Configuration,
) -> anyhow::Result<()> {
    let source = workspace_root.join("vendor/openvdb");
    require_file(&source.join("CMakeLists.txt"), "OpenVDB")?;
    let project = workspace_root.join("tools/native/cmake/openvdb");
    let destination = blackflower_build::vendor_directory(native_root, configuration, "openvdb");
    let boost = workspace_root.join("vendor/boost");
    let blosc = blackflower_build::vendor_directory(native_root, configuration, "blosc");
    let tbb = blackflower_build::vendor_directory(native_root, configuration, "tbb");
    let zlib = blackflower_build::vendor_directory(native_root, configuration, "zlib");
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
        .define("BLACKFLOWER_OPENVDB_ROOT", &source)
        .define("BLACKFLOWER_BOOST_ROOT", boost)
        .define("BLACKFLOWER_BLOSC_ROOT", blosc)
        .define("BLACKFLOWER_TBB_ROOT", tbb)
        .define("BLACKFLOWER_ZLIB_ROOT", zlib);
    let installed = config.build();
    write_vendor_manifest(&installed, configuration, Vendor::Openvdb, &source)
}
