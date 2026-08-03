use super::*;

pub(super) const VERSION: &str = "5.6.0";

pub(super) fn build(
    workspace_root: &Path,
    native_root: &Path,
    configuration: &Configuration,
) -> anyhow::Result<()> {
    let source = workspace_root.join("vendor/JoltPhysics");
    require_file(&source.join("Jolt/Jolt.h"), "Jolt Physics")?;
    let project = workspace_root.join("tools/native/cmake/jolt");
    let destination = blackflower_build::vendor_directory(native_root, configuration, "jolt");
    let (architecture, operating_system) = target_platform(&configuration.target)?;
    let mut config = base_config(
        &project,
        &destination,
        configuration,
        architecture,
        operating_system,
    );
    config
        .profile("Distribution")
        .build_target("install")
        .define_path("BLACKFLOWER_JOLT_ROOT", &source)
        .define("CROSS_PLATFORM_DETERMINISTIC", "ON")
        .define("DEBUG_RENDERER_IN_DEBUG_AND_RELEASE", "OFF")
        .define("DEBUG_RENDERER_IN_DISTRIBUTION", "OFF")
        .define("ENABLE_ALL_WARNINGS", "OFF")
        .define("ENABLE_INSTALL", "OFF")
        .define("ENABLE_OBJECT_STREAM", "OFF")
        .define("GENERATE_DEBUG_SYMBOLS", "OFF")
        .define("INTERPROCEDURAL_OPTIMIZATION", "OFF")
        .define("JPH_BUILD_SHARED_LIBS", "OFF")
        .define("JPH_USE_CPU_COMPUTE", "OFF")
        .define("JPH_USE_DX12", "OFF")
        .define("JPH_USE_MTL", "OFF")
        .define("JPH_USE_VK", "OFF")
        .define("PROFILER_IN_DEBUG_AND_RELEASE", "OFF")
        .define("PROFILER_IN_DISTRIBUTION", "OFF");
    configure_jolt_instruction_sets(&mut config, architecture, operating_system);
    let installed = config.build();
    write_vendor_manifest(&installed, configuration, Vendor::Jolt, &source)
}

fn configure_jolt_instruction_sets(
    config: &mut cmake::Config,
    architecture: &str,
    operating_system: &str,
) {
    for instruction_set in [
        "USE_AVX",
        "USE_AVX2",
        "USE_AVX512",
        "USE_F16C",
        "USE_FMADD",
        "USE_LZCNT",
        "USE_SSE4_1",
        "USE_SSE4_2",
        "USE_TZCNT",
    ] {
        let enabled = operating_system == "linux"
            && architecture == "x86_64"
            && instruction_set == "USE_AVX2";
        config.define(instruction_set, if enabled { "ON" } else { "OFF" });
    }
}
