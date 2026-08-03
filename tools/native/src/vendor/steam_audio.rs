use super::*;

pub(super) const VERSION: &str = "4.8.1";
const ISPC_VERSION: &str = super::embree::ISPC_VERSION;

struct EmbreeArtifacts {
    include: PathBuf,
    lexers: PathBuf,
    math: PathBuf,
    simd: PathBuf,
    sys: PathBuf,
    tasking: PathBuf,
    sse2: PathBuf,
    sse4: Option<PathBuf>,
    avx: Option<PathBuf>,
    avx2: Option<PathBuf>,
}

struct SteamAudioDependencies {
    embree: EmbreeArtifacts,
    flatbuffers: PathBuf,
    flatc: PathBuf,
    pffft: PathBuf,
    pffft_library: PathBuf,
    mysofa: PathBuf,
    mysofa_library: PathBuf,
    zlib: PathBuf,
    zlib_library: PathBuf,
    ispc: Option<PathBuf>,
    windows: bool,
}

pub(super) fn build(
    workspace_root: &Path,
    native_root: &Path,
    configuration: &Configuration,
) -> anyhow::Result<()> {
    let vendor_source = workspace_root.join("vendor/steam-audio-sdk/core");
    require_file(&vendor_source.join("CMakeLists.txt"), "Steam Audio")?;
    let destination =
        blackflower_build::vendor_directory(native_root, configuration, "steam-audio");
    let (architecture, operating_system) = target_platform(&configuration.target)?;
    let source =
        stage_steam_audio_source(&vendor_source, &destination, architecture, operating_system)?;
    let dependencies =
        load_steam_audio_dependencies(native_root, configuration, architecture, operating_system)?;
    let mut config = base_config(
        &source,
        &destination,
        configuration,
        architecture,
        operating_system,
    );
    configure_steam_audio_features(&mut config, configuration, architecture);
    configure_steam_audio_dependencies(&mut config, &dependencies);
    let built = config.build();
    install_steam_audio_artifacts(
        &built.join("build"),
        &destination,
        dependencies.windows,
        architecture,
    )?;
    write_vendor_manifest(
        &destination,
        configuration,
        Vendor::SteamAudio,
        &vendor_source,
    )
}

fn stage_steam_audio_source(
    vendor_source: &Path,
    destination: &Path,
    architecture: &str,
    operating_system: &str,
) -> anyhow::Result<PathBuf> {
    let source = destination.join("source");
    if source.exists() {
        fs::remove_dir_all(&source)?;
    }
    copy_tree(vendor_source, &source)?;
    patch_steam_audio_linux_abi(&source, operating_system)?;
    patch_steam_audio_embree_include(&source)?;
    patch_steam_audio_embree_scene_loading(&source)?;
    patch_steam_audio_ispc_version(&source)?;
    patch_steam_audio_embree_arm64(&source, architecture)?;
    Ok(source)
}

fn load_steam_audio_dependencies(
    native_root: &Path,
    configuration: &Configuration,
    architecture: &str,
    operating_system: &str,
) -> anyhow::Result<SteamAudioDependencies> {
    let directory =
        |vendor| blackflower_build::vendor_directory(native_root, configuration, vendor);
    let embree_root = directory("embree");
    let flatbuffers = directory("flatbuffers");
    let pffft = directory("pffft");
    let mysofa = directory("mysofa");
    let zlib = directory("zlib");
    let windows = operating_system == "windows";
    Ok(SteamAudioDependencies {
        embree: load_embree_artifacts(&embree_root, architecture, operating_system)?,
        flatc: find_built_file(&flatbuffers, if windows { "flatc.exe" } else { "flatc" })?,
        pffft_library: find_static_library(&pffft, windows, "pffft", "pffft")?,
        mysofa_library: find_static_library(&mysofa, windows, "mysofa", "mysofa")?,
        zlib_library: find_static_library(&zlib, windows, "z", "zlibstatic")?,
        ispc: (architecture == "x86_64")
            .then(|| embree::find_ispc(operating_system))
            .transpose()?,
        flatbuffers,
        pffft,
        mysofa,
        zlib,
        windows,
    })
}

fn configure_steam_audio_features(
    config: &mut cmake::Config,
    configuration: &Configuration,
    architecture: &str,
) {
    config
        .build_target("phonon")
        .define("BUILD_SHARED_LIBS", "OFF")
        .define(
            "STEAMAUDIO_STATIC_RUNTIME",
            if configuration.crt_static {
                "ON"
            } else {
                "OFF"
            },
        )
        .define("STEAMAUDIO_BUILD_TESTS", "OFF")
        .define("STEAMAUDIO_BUILD_ITESTS", "OFF")
        .define("STEAMAUDIO_BUILD_BENCHMARKS", "OFF")
        .define("STEAMAUDIO_BUILD_SAMPLES", "OFF")
        .define("STEAMAUDIO_BUILD_DOCS", "OFF")
        .define("STEAMAUDIO_ENABLE_IPP", "OFF")
        .define("STEAMAUDIO_ENABLE_MKL", "OFF")
        .define("STEAMAUDIO_ENABLE_EMBREE", "ON")
        .define("STEAMAUDIO_ENABLE_FFTS", "OFF")
        .define("STEAMAUDIO_ENABLE_RADEONRAYS", "OFF")
        .define("STEAMAUDIO_ENABLE_TRUEAUDIONEXT", "OFF");
    if architecture == "aarch64" {
        config.define("BLACKFLOWER_EMBREE_CPP_REFLECTION", "ON");
    }
}

fn configure_steam_audio_dependencies(
    config: &mut cmake::Config,
    dependencies: &SteamAudioDependencies,
) {
    let embree = &dependencies.embree;
    config
        .define_path(
            "FlatBuffers_INCLUDE_DIR",
            dependencies.flatbuffers.join("include"),
        )
        .define_path("FlatBuffers_EXECUTABLE", &dependencies.flatc)
        .define_path("PFFFT_INCLUDE_DIR", dependencies.pffft.join("include"))
        .define_path("PFFFT_LIBRARY", &dependencies.pffft_library)
        .define_path("MySOFA_INCLUDE_DIR", dependencies.mysofa.join("include"))
        .define_path("MySOFA_LIBRARY", &dependencies.mysofa_library)
        .define_path("ZLIB_INCLUDE_DIR", dependencies.zlib.join("include"))
        .define_path("ZLIB_LIBRARY", &dependencies.zlib_library)
        .define_path("Embree_INCLUDE_DIR", &embree.include)
        .define_path("Embree_lexers_LIBRARY", &embree.lexers)
        .define_path("Embree_math_LIBRARY", &embree.math)
        .define_path("Embree_simd_LIBRARY", &embree.simd)
        .define_path("Embree_sys_LIBRARY", &embree.sys)
        .define_path("Embree_tasking_LIBRARY", &embree.tasking)
        .define_path("Embree_sse2_LIBRARY", &embree.sse2);
    for (name, library) in [
        ("Embree_sse4_LIBRARY", &embree.sse4),
        ("Embree_avx_LIBRARY", &embree.avx),
        ("Embree_avx2_LIBRARY", &embree.avx2),
    ] {
        if let Some(library) = library {
            config.define_path(name, library);
        }
    }
    if let Some(executable) = &dependencies.ispc {
        config
            .define_path("ISPC_EXECUTABLE", executable)
            .define("ISPC_VERSION", ISPC_VERSION);
    }
}

fn install_steam_audio_artifacts(
    built: &Path,
    destination: &Path,
    windows: bool,
    architecture: &str,
) -> anyhow::Result<()> {
    let library_dir = destination.join("lib");
    fs::create_dir_all(&library_dir)?;
    copy_static_artifact(
        &find_static_library(built, windows, "phonon", "phonon")?,
        &library_dir,
    )?;
    if architecture == "x86_64" {
        copy_static_artifact(
            &find_static_library(built, windows, "ispckernels", "ispckernels")?,
            &library_dir,
        )?;
    }
    Ok(())
}

fn load_embree_artifacts(
    root: &Path,
    architecture: &str,
    operating_system: &str,
) -> anyhow::Result<EmbreeArtifacts> {
    let windows = operating_system == "windows";
    let has_x86_variants = architecture == "x86_64" && operating_system != "macos";
    let has_avx2 = has_x86_variants || (architecture == "aarch64" && operating_system == "macos");
    Ok(EmbreeArtifacts {
        include: root.join("include/embree4"),
        lexers: find_static_library(root, windows, "lexers", "lexers")?,
        math: find_static_library(root, windows, "math", "math")?,
        simd: find_static_library(root, windows, "simd", "simd")?,
        sys: find_static_library(root, windows, "sys", "sys")?,
        tasking: find_static_library(root, windows, "tasking", "tasking")?,
        sse2: find_static_library(root, windows, "embree", "embree")?,
        sse4: has_x86_variants
            .then(|| find_static_library(root, windows, "embree_sse42", "embree_sse42"))
            .transpose()?,
        avx: has_x86_variants
            .then(|| find_static_library(root, windows, "embree_avx", "embree_avx"))
            .transpose()?,
        avx2: has_avx2
            .then(|| find_static_library(root, windows, "embree_avx2", "embree_avx2"))
            .transpose()?,
    })
}

fn copy_static_artifact(source: &Path, destination: &Path) -> anyhow::Result<()> {
    let file_name = source
        .file_name()
        .context("static library artifact has no file name")?;
    let installed = destination.join(file_name);
    if source == installed {
        bail!(
            "refusing to copy static library {} onto itself",
            source.display()
        );
    }
    fs::copy(source, &installed)?;
    if fs::metadata(&installed)?.len() == 0 {
        bail!("installed static library {} is empty", installed.display());
    }
    Ok(())
}

fn patch_steam_audio_ispc_version(source: &Path) -> anyhow::Result<()> {
    replace_exact(
        &source.join("CMakeLists.txt"),
        "find_package(ISPC 1.12 EXACT)",
        "find_package(ISPC 1.31 EXACT)",
        "Steam Audio ISPC version contract changed",
    )
}

fn patch_steam_audio_embree_include(source: &Path) -> anyhow::Result<()> {
    replace_exact(
        &source.join("src/core/CMakeLists.txt"),
        "    set(ISPC_FLAGS          -I ${CMAKE_HOME_DIRECTORY}/deps/embree/include -g)",
        "    set(ISPC_FLAGS          -I ${Embree_INCLUDE_DIR} -g)",
        "Steam Audio Embree ISPC include path contract changed",
    )?;
    replace_exact(
        &source.join("src/core/embree_device.cpp"),
        "    mDevice = rtcNewDevice(nullptr);",
        "    mDevice = rtcNewDevice(\"set_affinity=0\");",
        "Steam Audio Embree device configuration contract changed",
    )
}

#[allow(
    clippy::too_many_lines,
    reason = "the pinned-source patch keeps every exact Steam Audio scene-loading contract together"
)]
fn patch_steam_audio_embree_scene_loading(source: &Path) -> anyhow::Result<()> {
    let header_path = source.join("src/core/embree_static_mesh.h");
    replace_exact(
        &header_path,
        r#"    EmbreeStaticMesh(shared_ptr<EmbreeScene> scene,
                     const Serialized::StaticMesh* serializedObject);

    EmbreeStaticMesh(shared_ptr<EmbreeScene> scene,
                     SerializedObject& serializedObject);"#,
        r#"    EmbreeStaticMesh(shared_ptr<EmbreeScene> scene,
                     const Serialized::StaticMesh* serializedObject);

    // A deserialized scene owns this mesh directly, before shared_from_this() is available.
    EmbreeStaticMesh(EmbreeScene& scene,
                     const Serialized::StaticMesh* serializedObject);

    EmbreeStaticMesh(shared_ptr<EmbreeScene> scene,
                     SerializedObject& serializedObject);"#,
        "Steam Audio Embree static mesh constructor contract changed",
    )?;
    replace_exact(
        &header_path,
        r#"    void initialize(const EmbreeScene& scene,
                    const Vector3f* vertices,
                    const Triangle* triangles);

    void convertMaterials();

    std::weak_ptr<EmbreeScene> mScene;
    RTCGeometry mGeometry;"#,
        r#"    void initialize(const EmbreeScene& scene,
                    const Vector3f* vertices,
                    const Triangle* triangles);

    void initialize(const EmbreeScene& scene,
                    const Serialized::StaticMesh* serializedObject);

    void convertMaterials();

    std::weak_ptr<EmbreeScene> mScene;
    EmbreeScene* mOwningScene = nullptr;
    RTCGeometry mGeometry;"#,
        "Steam Audio Embree static mesh member contract changed",
    )?;

    let static_mesh_path = source.join("src/core/embree_static_mesh.cpp");
    replace_exact(
        &static_mesh_path,
        r#"EmbreeStaticMesh::EmbreeStaticMesh(shared_ptr<EmbreeScene> scene,
                                   const Serialized::StaticMesh* serializedObject)
    : mScene(scene)
{
    assert(serializedObject);"#,
        r#"EmbreeStaticMesh::EmbreeStaticMesh(shared_ptr<EmbreeScene> scene,
                                   const Serialized::StaticMesh* serializedObject)
    : mScene(scene)
{
    initialize(*scene, serializedObject);
}

EmbreeStaticMesh::EmbreeStaticMesh(EmbreeScene& scene,
                                   const Serialized::StaticMesh* serializedObject)
    : mOwningScene(&scene)
{
    initialize(scene, serializedObject);
}

void EmbreeStaticMesh::initialize(const EmbreeScene& scene,
                                  const Serialized::StaticMesh* serializedObject)
{
    assert(serializedObject);"#,
        "Steam Audio Embree serialized mesh constructor contract changed",
    )?;
    replace_exact(
        &static_mesh_path,
        "    initialize(*scene, vertices.data(), triangles.data());",
        "    initialize(scene, vertices.data(), triangles.data());",
        "Steam Audio Embree serialized mesh initialization contract changed",
    )?;
    replace_exact(
        &static_mesh_path,
        r#"EmbreeStaticMesh::~EmbreeStaticMesh()
{
    if (auto scene = mScene.lock())
    {
        scene->releaseGeometryID(mGeometryIndex);
        rtcReleaseGeometry(mGeometry);
    }
}"#,
        r#"EmbreeStaticMesh::~EmbreeStaticMesh()
{
    if (auto scene = mScene.lock())
    {
        scene->releaseGeometryID(mGeometryIndex);
    }
    else if (mOwningScene)
    {
        mOwningScene->releaseGeometryID(mGeometryIndex);
    }

    rtcReleaseGeometry(mGeometry);
}"#,
        "Steam Audio Embree static mesh destructor contract changed",
    )?;

    let scene_path = source.join("src/core/embree_scene.cpp");
    replace_exact(
        &scene_path,
        "        auto staticMesh = ipl::make_shared<EmbreeStaticMesh>(std::static_pointer_cast<EmbreeScene>(shared_from_this()), serializedObject->static_meshes()->Get(i));",
        "        auto staticMesh = ipl::make_shared<EmbreeStaticMesh>(*this, serializedObject->static_meshes()->Get(i));",
        "Steam Audio Embree serialized scene construction contract changed",
    )?;
    replace_exact(
        &scene_path,
        r#"EmbreeScene::~EmbreeScene()
{
    rtcReleaseScene(mScene);
}"#,
        r#"EmbreeScene::~EmbreeScene()
{
    // Scene-owned deserialized meshes must be destroyed while the parent is still valid.
    mStaticMeshes[0].clear();
    mStaticMeshes[1].clear();
    rtcReleaseScene(mScene);
}"#,
        "Steam Audio Embree scene destructor contract changed",
    )
}

fn replace_exact(
    path: &Path,
    original: &str,
    replacement: &str,
    contract_error: &str,
) -> anyhow::Result<()> {
    let contents = fs::read_to_string(path)?;
    if contents.matches(original).count() != 1 {
        bail!("{contract_error}");
    }
    fs::write(path, contents.replacen(original, replacement, 1))?;
    Ok(())
}

fn replace_all_checked(
    path: &Path,
    original: &str,
    replacement: &str,
    expected: usize,
    contract_error: &str,
) -> anyhow::Result<()> {
    let contents = fs::read_to_string(path)?;
    let occurrences = contents.matches(original).count();
    if occurrences != expected {
        bail!("{contract_error}: expected {expected}, found {occurrences}");
    }
    fs::write(path, contents.replace(original, replacement))?;
    Ok(())
}

fn patch_steam_audio_linux_abi(source: &Path, operating_system: &str) -> anyhow::Result<()> {
    if operating_system != "linux" {
        return Ok(());
    }
    const LEGACY_ABI_OPTION: &str = "        add_compile_options(-fabi-version=6)\n";
    replace_exact(
        &source.join("CMakeLists.txt"),
        LEGACY_ABI_OPTION,
        "",
        "Steam Audio legacy GCC ABI contract changed",
    )
}

#[allow(
    clippy::too_many_lines,
    reason = "the pinned-source patch keeps Steam Audio's ARM64 Embree port auditable"
)]
fn patch_steam_audio_embree_arm64(source: &Path, architecture: &str) -> anyhow::Result<()> {
    if architecture != "aarch64" {
        return Ok(());
    }

    const X86_GUARD: &str =
        "#if defined(IPL_USES_EMBREE) && (defined(IPL_CPU_X86) || defined(IPL_CPU_X64))";
    const ARM64_GUARD: &str = "#if defined(IPL_USES_EMBREE) && (defined(IPL_CPU_X86) || defined(IPL_CPU_X64) || defined(IPL_CPU_ARM64))";
    let core = source.join("src/core");
    for (file, expected) in [
        ("api_embree_device.cpp", 4),
        ("embree_device.cpp", 1),
        ("embree_device.h", 1),
        ("embree_instanced_mesh.cpp", 1),
        ("embree_instanced_mesh.h", 1),
        ("embree_scene.cpp", 1),
        ("embree_scene.h", 1),
        ("embree_static_mesh.cpp", 1),
        ("embree_static_mesh.h", 1),
        ("pch.h", 1),
        ("scene_factory.cpp", 2),
    ] {
        replace_all_checked(
            &core.join(file),
            X86_GUARD,
            ARM64_GUARD,
            expected,
            "Steam Audio Embree architecture guard contract changed",
        )?;
    }

    let root_cmake = source.join("CMakeLists.txt");
    replace_exact(
        &root_cmake,
        "    set(CMAKE_OSX_ARCHITECTURES \"x86_64;arm64\")\n    set(CMAKE_OSX_DEPLOYMENT_TARGET \"10.13\")",
        "    # The embedding build supplies one target architecture and deployment target.",
        "Steam Audio macOS target contract changed",
    )?;
    replace_exact(
        &root_cmake,
        r#"if (STEAMAUDIO_ENABLE_EMBREE)
    find_package(ISPC 1.31 EXACT)
    find_package(Embree 4)
    if (NOT ISPC_FOUND OR NOT Embree_FOUND)
        message(STATUS "Disabling Embree")
        set(STEAMAUDIO_ENABLE_EMBREE OFF)
    endif()
endif()"#,
        r#"if (STEAMAUDIO_ENABLE_EMBREE)
    if (NOT BLACKFLOWER_EMBREE_CPP_REFLECTION)
        find_package(ISPC 1.31 EXACT)
    endif()
    find_package(Embree 4)
    if ((NOT BLACKFLOWER_EMBREE_CPP_REFLECTION AND NOT ISPC_FOUND) OR NOT Embree_FOUND)
        message(STATUS "Disabling Embree")
        set(STEAMAUDIO_ENABLE_EMBREE OFF)
    endif()
endif()"#,
        "Steam Audio Embree dependency discovery contract changed",
    )?;

    let core_cmake = core.join("CMakeLists.txt");
    replace_exact(
        &core_cmake,
        "if (STEAMAUDIO_ENABLE_EMBREE)\n    if (WIN32)",
        "if (STEAMAUDIO_ENABLE_EMBREE AND NOT BLACKFLOWER_EMBREE_CPP_REFLECTION)\n    if (WIN32)",
        "Steam Audio Embree ISPC build contract changed",
    )?;
    replace_exact(
        &core_cmake,
        r#"if (STEAMAUDIO_ENABLE_EMBREE)
    target_sources(core PRIVATE
        embree_device.h
        embree_device.cpp
        embree_static_mesh.h
        embree_static_mesh.cpp
        embree_instanced_mesh.h
        embree_instanced_mesh.cpp
        embree_scene.h
        embree_scene.cpp
        embree_reflection_simulator.h
        embree_reflection_simulator.cpp
        embree_reflection_simulator.ispc
    )
endif()"#,
        r#"if (STEAMAUDIO_ENABLE_EMBREE)
    target_sources(core PRIVATE
        embree_device.h
        embree_device.cpp
        embree_static_mesh.h
        embree_static_mesh.cpp
        embree_instanced_mesh.h
        embree_instanced_mesh.cpp
        embree_scene.h
        embree_scene.cpp
    )
    if (NOT BLACKFLOWER_EMBREE_CPP_REFLECTION)
        target_sources(core PRIVATE
            embree_reflection_simulator.h
            embree_reflection_simulator.cpp
            embree_reflection_simulator.ispc
        )
    endif()
endif()"#,
        "Steam Audio Embree source list contract changed",
    )?;
    replace_exact(
        &core_cmake,
        r#"if (STEAMAUDIO_ENABLE_EMBREE)
    target_link_libraries(core PUBLIC Embree::Embree ispckernels)
endif()"#,
        r#"if (STEAMAUDIO_ENABLE_EMBREE)
    target_link_libraries(core PUBLIC Embree::Embree)
    if (NOT BLACKFLOWER_EMBREE_CPP_REFLECTION)
        target_link_libraries(core PUBLIC ispckernels)
    endif()
endif()"#,
        "Steam Audio Embree link contract changed",
    )?;

    let find_embree = source.join("build/FindEmbree.cmake");
    replace_all_checked(
        &find_embree,
        "if (NOT IPL_OS_MACOS)",
        "if (NOT IPL_OS_MACOS AND NOT IPL_CPU_ARMV8)",
        3,
        "Steam Audio Embree ISA library discovery contract changed",
    )?;
    replace_all_checked(
        &find_embree,
        "if (IPL_OS_MACOS)",
        "if (IPL_OS_MACOS OR IPL_CPU_ARMV8)",
        2,
        "Steam Audio Embree base library contract changed",
    )?;

    let scene_header = core.join("embree_scene.h");
    replace_exact(
        &scene_header,
        "#include \"embree_reflection_simulator.ispc.h\"",
        "#if defined(IPL_CPU_X86) || defined(IPL_CPU_X64)\n#include \"embree_reflection_simulator.ispc.h\"\n#endif",
        "Steam Audio Embree scene ISPC include contract changed",
    )?;
    replace_exact(
        &scene_header,
        r#"    const ispc::Material* const* ispcMaterialsForGeometry() const
    {
        return mISPCMaterialsForGeometry.data();
    }
"#,
        r#"#if defined(IPL_CPU_X86) || defined(IPL_CPU_X64)
    const ispc::Material* const* ispcMaterialsForGeometry() const
    {
        return mISPCMaterialsForGeometry.data();
    }
#endif
"#,
        "Steam Audio Embree scene ISPC accessor contract changed",
    )?;
    replace_exact(
        &scene_header,
        "    vector<const ispc::Material*> mISPCMaterialsForGeometry;",
        "#if defined(IPL_CPU_X86) || defined(IPL_CPU_X64)\n    vector<const ispc::Material*> mISPCMaterialsForGeometry;\n#endif",
        "Steam Audio Embree scene ISPC storage contract changed",
    )?;

    let static_mesh_header = core.join("embree_static_mesh.h");
    replace_exact(
        &static_mesh_header,
        r#"    ispc::Material* ispcMaterials()
    {
        return mISPCMaterials.data();
    }

    const ispc::Material* ispcMaterials() const
    {
        return mISPCMaterials.data();
    }
"#,
        r#"#if defined(IPL_CPU_X86) || defined(IPL_CPU_X64)
    ispc::Material* ispcMaterials()
    {
        return mISPCMaterials.data();
    }

    const ispc::Material* ispcMaterials() const
    {
        return mISPCMaterials.data();
    }
#endif
"#,
        "Steam Audio Embree static mesh ISPC accessor contract changed",
    )?;
    replace_exact(
        &static_mesh_header,
        "    vector<ispc::Material> mISPCMaterials;",
        "#if defined(IPL_CPU_X86) || defined(IPL_CPU_X64)\n    vector<ispc::Material> mISPCMaterials;\n#endif",
        "Steam Audio Embree static mesh ISPC storage contract changed",
    )?;

    let static_mesh = core.join("embree_static_mesh.cpp");
    replace_exact(
        &static_mesh,
        r#"void EmbreeStaticMesh::convertMaterials()
{
    mISPCMaterials.resize(mMaterials.size(0));

    for (auto i = 0; i < mMaterials.size(0); ++i)
    {
        mISPCMaterials[i].absorption = mMaterials[i].absorption;
        mISPCMaterials[i].scattering = mMaterials[i].scattering;
        mISPCMaterials[i].transmission = mMaterials[i].transmission;
    }
}"#,
        r#"void EmbreeStaticMesh::convertMaterials()
{
#if defined(IPL_CPU_X86) || defined(IPL_CPU_X64)
    mISPCMaterials.resize(mMaterials.size(0));

    for (auto i = 0; i < mMaterials.size(0); ++i)
    {
        mISPCMaterials[i].absorption = mMaterials[i].absorption;
        mISPCMaterials[i].scattering = mMaterials[i].scattering;
        mISPCMaterials[i].transmission = mMaterials[i].transmission;
    }
#endif
}"#,
        "Steam Audio Embree material conversion contract changed",
    )?;

    let scene = core.join("embree_scene.cpp");
    replace_exact(
        &scene,
        "    mISPCMaterialsForGeometry.resize(maxID + 1);",
        "#if defined(IPL_CPU_X86) || defined(IPL_CPU_X64)\n    mISPCMaterialsForGeometry.resize(maxID + 1);\n#endif",
        "Steam Audio Embree scene ISPC resize contract changed",
    )?;
    replace_all_checked(
        &scene,
        "        mISPCMaterialsForGeometry[index] = embreeStaticMesh->ispcMaterials();",
        "#if defined(IPL_CPU_X86) || defined(IPL_CPU_X64)\n        mISPCMaterialsForGeometry[index] = embreeStaticMesh->ispcMaterials();\n#endif",
        2,
        "Steam Audio Embree scene ISPC assignment contract changed",
    )?;

    let reflection_factory = core.join("reflection_simulator_factory.cpp");
    replace_exact(
        &reflection_factory,
        r#"#if defined(IPL_USES_EMBREE) && (defined(IPL_CPU_X86) || defined(IPL_CPU_X64))
    case SceneType::Embree:
        return ipl::make_unique<EmbreeReflectionSimulator>(maxNumRays, numDiffuseSamples, maxDuration, maxOrder, maxNumSources,
                                                           numThreads);
#endif"#,
        r#"#if defined(IPL_USES_EMBREE) && (defined(IPL_CPU_X86) || defined(IPL_CPU_X64))
    case SceneType::Embree:
        return ipl::make_unique<EmbreeReflectionSimulator>(maxNumRays, numDiffuseSamples, maxDuration, maxOrder, maxNumSources,
                                                           numThreads);
#elif defined(IPL_USES_EMBREE) && defined(IPL_CPU_ARM64)
    case SceneType::Embree:
        return ipl::make_unique<ReflectionSimulator>(maxNumRays, numDiffuseSamples, maxDuration, maxOrder, maxNumSources,
                                                     numThreads);
#endif"#,
        "Steam Audio Embree reflection simulator factory contract changed",
    )
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn static_artifact_self_copy_is_rejected_without_truncating() -> anyhow::Result<()> {
        let temporary = tempfile::tempdir()?;
        let library = temporary.path().join("libispckernels.a");
        let archive_header = b"!<arch>\n";
        fs::write(&library, archive_header)?;

        let error = match copy_static_artifact(&library, temporary.path()) {
            Ok(()) => bail!("self-copy unexpectedly succeeded"),
            Err(error) => error,
        };

        assert!(error.to_string().contains("onto itself"));
        assert_eq!(fs::read(library)?, archive_header);
        Ok(())
    }
}
