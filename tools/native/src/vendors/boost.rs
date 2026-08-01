use super::*;

pub(super) const VERSION: &str = "1.85.0";

pub(super) fn build(
    workspace_root: &Path,
    native_root: &Path,
    configuration: &Configuration,
) -> anyhow::Result<()> {
    let source = workspace_root.join("vendor/boost");
    require_file(
        &source.join("libs/interprocess/include/boost/interprocess/file_mapping.hpp"),
        "Boost.Interprocess header",
    )?;
    require_file(
        &source.join("libs/iostreams/include/boost/iostreams/copy.hpp"),
        "Boost.Iostreams header",
    )?;
    let destination = native_vendors::vendor_directory(native_root, configuration, "boost");
    fs::create_dir_all(&destination)?;
    write_vendor_manifest(&destination, configuration, Vendor::Boost, &source)
}
