use super::*;

pub(super) const VERSION: &str = "0.731.0";
const LUA_USE_LONGJMP: &str = "1";
const LUA_IDSIZE: &str = "256";
const LUA_VECTOR_SIZE: &str = "3";
const LUA_VECTOR_DOUBLE: &str = "0";

fn abi_contract() -> String {
    format!(
        "longjmp={LUA_USE_LONGJMP};idsize={LUA_IDSIZE};vector_size={LUA_VECTOR_SIZE};vector_double={LUA_VECTOR_DOUBLE}"
    )
}

pub(super) fn build(
    workspace_root: &Path,
    native_root: &Path,
    configuration: &Configuration,
) -> anyhow::Result<()> {
    let source = workspace_root.join("vendor/luau");
    require_file(&source.join("CMakeLists.txt"), "Luau")?;
    let project = workspace_root.join("tools/native/cmake/luau");
    let destination = blackflower_build::vendor_directory(native_root, configuration, "luau");
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
        .define_path("BLACKFLOWER_LUAU_ROOT", &source)
        .define("BLACKFLOWER_LUA_USE_LONGJMP", LUA_USE_LONGJMP)
        .define("BLACKFLOWER_LUA_IDSIZE", LUA_IDSIZE)
        .define("BLACKFLOWER_LUA_VECTOR_SIZE", LUA_VECTOR_SIZE)
        .define("BLACKFLOWER_LUA_VECTOR_DOUBLE", LUA_VECTOR_DOUBLE)
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
    write_vendor_manifest(&installed, configuration, Vendor::Luau, &source)?;
    blackflower_build::write_vendor_manifest_field(&installed, "luau_abi", &abi_contract())
        .map_err(|error| anyhow::anyhow!(error.to_string()))
}
