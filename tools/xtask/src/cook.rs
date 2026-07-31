use std::collections::BTreeMap;
use std::fs::{self, File};
use std::io::{Cursor, Write};
use std::path::{Path, PathBuf};

use anyhow::{Context, bail};
use backhand::compression::{CompressionOptions, Compressor, Zstd};
use backhand::{FilesystemCompressor, FilesystemWriter, NodeHeader};
use blackflower_assets::{
    ASSET_CATALOG_SCHEMA, AssetCatalog, AssetId, AssetPackage, AssetRecord, AssetSigningKey,
    AssetTrustStore, Bytes, ContentHash, CookingProfileIdentity, PackageHash, PackageName,
    ProfileName, RecipeHash, ToolchainIdentity, sign_package,
};
use blackflower_scripting::luau_version;
use serde::Serialize;
use tempfile::{NamedTempFile, TempDir};

use crate::asset_cooker::{
    CookedAsset, HALF_VERSION, IMAGE_VERSION, NAGA_VERSION, cook_assets, texture_encoder_platform,
};
use crate::gltf_source;
use crate::manifest::{Repository, SOURCE_SCHEMA};
use crate::mesh_cooker::MESHOPT_VERSION;
use crate::navigation_cooker::platform_identity as navigation_cooker_platform;
use crate::profile::CookingProfiles;

const BLOCK_SIZE: u32 = 128 * 1024;
const ZSTD_LEVEL: u32 = 3;
const DIR_MODE: u16 = 0o555;
const FILE_MODE: u16 = 0o444;

#[derive(Debug)]
pub(crate) struct Pipeline {
    profiles_root: PathBuf,
    source_root: PathBuf,
    target_root: PathBuf,
}

#[derive(Debug)]
pub(crate) struct CookRequest {
    pub(crate) profile: ProfileName,
    pub(crate) package: PackageName,
    pub(crate) signing_key: AssetSigningKey,
}

#[derive(Debug)]
pub(crate) struct CookResult {
    pub(crate) path: PathBuf,
    pub(crate) package_hash: PackageHash,
    pub(crate) assets: usize,
}

#[derive(Debug)]
pub(crate) struct CheckResult {
    pub(crate) profiles: usize,
    pub(crate) assets: usize,
    pub(crate) gltf_sources: usize,
    pub(crate) packages: usize,
}

impl Pipeline {
    pub(crate) fn for_workspace(workspace_root: &Path) -> Self {
        Self {
            profiles_root: workspace_root.join("assets/profiles"),
            source_root: workspace_root.join("assets/source"),
            target_root: workspace_root.join("target"),
        }
    }

    #[cfg(test)]
    fn new(profiles_root: PathBuf, source_root: PathBuf, target_root: PathBuf) -> Self {
        Self {
            profiles_root,
            source_root,
            target_root,
        }
    }

    pub(crate) fn check(&self) -> anyhow::Result<CheckResult> {
        let profiles = CookingProfiles::load(&self.profiles_root)?;
        let gltf = gltf_source::validate_tree(&self.source_root)?;
        let repository = Repository::load(&self.source_root)?;
        Ok(CheckResult {
            profiles: profiles.len(),
            assets: repository.assets.len(),
            gltf_sources: gltf.sources,
            packages: repository.packages.len(),
        })
    }

    pub(crate) fn cook(&self, request: &CookRequest) -> anyhow::Result<CookResult> {
        let profiles = CookingProfiles::load(&self.profiles_root)?;
        let profile = profiles.get(&request.profile)?;
        let _validated_gltf = gltf_source::validate_tree(&self.source_root)?;
        let repository = Repository::load(&self.source_root)?;
        let selected = repository.selected_assets(&request.package)?;
        let toolchain = toolchain_identity();
        let cooked = cook_assets(&repository, &selected, profile)?;
        let catalog = build_catalog(&cooked, profile.identity.clone(), toolchain)?;
        self.populate_cache(&cooked, &catalog)?;
        let package_dir = self
            .target_root
            .join("assets/packages")
            .join(request.profile.as_str());
        fs::create_dir_all(&package_dir)
            .with_context(|| format!("failed to create `{}`", package_dir.display()))?;
        let output_path = package_dir.join(request.package.file_name());
        let package_hash = write_and_publish_package(
            &package_dir,
            &output_path,
            &catalog,
            &cooked,
            &request.signing_key,
        )?;
        Ok(CookResult {
            path: output_path,
            package_hash,
            assets: catalog.assets.len(),
        })
    }

    fn populate_cache(
        &self,
        cooked: &BTreeMap<AssetId, CookedAsset>,
        catalog: &AssetCatalog,
    ) -> anyhow::Result<()> {
        let object_root = self.target_root.join("asset-cache/objects/blake3");
        let recipe_root = self.target_root.join("asset-cache/recipes");
        fs::create_dir_all(&object_root)?;
        fs::create_dir_all(&recipe_root)?;
        for record in &catalog.assets {
            let asset = cooked
                .get(&record.id)
                .with_context(|| format!("missing cooked asset `{}`", record.id))?;
            let object_path = object_root.join(record.content_hash.to_string());
            write_cache_object(&object_path, &asset.bytes, record.content_hash)?;
            let recipe_path = recipe_root.join(format!("{}.json", record.recipe_hash));
            let recipe = CachedRecipe {
                schema: SOURCE_SCHEMA,
                id: &record.id,
                content_hash: record.content_hash,
                recipe_hash: record.recipe_hash,
            };
            let mut bytes = serde_json::to_vec(&recipe)?;
            bytes.push(b'\n');
            write_atomic(&recipe_path, &bytes)?;
        }
        Ok(())
    }
}

fn toolchain_identity() -> ToolchainIdentity {
    let (major, minor, patch) = luau_version();
    ToolchainIdentity {
        cooker: format!("xtask/{}", env!("CARGO_PKG_VERSION")),
        squashfs: "backhand/0.25.1".to_owned(),
        archive:
            "squashfs-4.0-le;block=131072;zstd=3;epoch=0;uid=0;gid=0;signature=ed25519-blake3-v1"
                .to_owned(),
        luau: format!("luau/{major}.{minor}.{patch}"),
        slang: format!("slang/{}", blackflower_shader_compiler::slang_version()),
        naga: format!("naga/{NAGA_VERSION}"),
        ktx: format!("ktx/{}", blackflower_rendering_textures::ktx_version()),
        texture_decoder: format!("image/{IMAGE_VERSION}+half/{HALF_VERSION}"),
        texture_encoder_platform: texture_encoder_platform(),
        meshoptimizer: format!("meshopt/{MESHOPT_VERSION}"),
        ozz_animation: format!(
            "ozz/{}@{};bf-container={}",
            blackflower_cooker_animation::OZZ_VERSION,
            blackflower_cooker_animation::OZZ_REVISION,
            blackflower_animation_format::CONTAINER_SCHEMA,
        ),
        openvdb: format!(
            "openvdb/{}@{}",
            blackflower_cooker_volume::OPENVDB_VERSION,
            blackflower_cooker_volume::OPENVDB_REVISION,
        ),
        nanovdb: format!(
            "nanovdb/{};{}",
            blackflower_cooker_volume::NANOVDB_VERSION,
            blackflower_cooker_volume::COOKER_RECIPE,
        ),
        boost: format!("boost/{}", blackflower_cooker_volume::BOOST_VERSION),
        one_tbb: format!("oneTBB/{}", blackflower_cooker_volume::ONE_TBB_VERSION),
        blosc: format!("c-blosc/{}", blackflower_cooker_volume::BLOSC_VERSION),
        zlib: format!("zlib/{}", blackflower_cooker_volume::ZLIB_VERSION),
        recast_navigation: format!(
            "recastnavigation/{}@{};bfnav={};{}",
            blackflower_cooker_navigation::RECAST_VERSION,
            blackflower_cooker_navigation::RECAST_REVISION,
            blackflower_navigation::NAVIGATION_ASSET_SCHEMA,
            blackflower_cooker_navigation::COOKER_RECIPE,
        ),
        navigation_cooker_platform: navigation_cooker_platform(),
    }
}

fn build_catalog(
    cooked: &BTreeMap<AssetId, CookedAsset>,
    profile: CookingProfileIdentity,
    toolchain: ToolchainIdentity,
) -> anyhow::Result<AssetCatalog> {
    let mut assets = Vec::with_capacity(cooked.len());
    for (id, asset) in cooked {
        let byte_len = u64::try_from(asset.bytes.len()).context("asset length does not fit u64")?;
        assets.push(AssetRecord {
            id: id.clone(),
            kind: asset.kind,
            audience: asset.audience,
            dependencies: asset.dependencies.clone(),
            content_hash: asset.content_hash,
            recipe_hash: asset.recipe_hash,
            byte_len,
            object_path: format!("objects/blake3/{}", asset.content_hash),
        });
    }
    Ok(AssetCatalog {
        schema: ASSET_CATALOG_SCHEMA,
        profile,
        toolchain,
        assets,
    })
}

fn write_and_publish_package(
    package_dir: &Path,
    output_path: &Path,
    catalog: &AssetCatalog,
    cooked: &BTreeMap<AssetId, CookedAsset>,
    signing_key: &AssetSigningKey,
) -> anyhow::Result<PackageHash> {
    let staging = TempDir::new_in(package_dir).with_context(|| {
        format!(
            "failed to create staging directory in `{}`",
            package_dir.display()
        )
    })?;
    let file_name = output_path
        .file_name()
        .context("package output path has no filename")?;
    let candidate = staging.path().join(file_name);
    write_package(&candidate, catalog, cooked)?;
    let _payload_hash = sign_package(&candidate, signing_key)?;
    let trust_store = AssetTrustStore::from_public_keys([signing_key.public_key_bytes()])?;
    let package = AssetPackage::open(&candidate, &trust_store)?;
    if package.catalog() != catalog {
        bail!("staged package catalog differs from the cooker catalog");
    }
    let package_hash = package.hash();
    drop(package);
    replace_output(&candidate, output_path, staging.path())?;
    Ok(package_hash)
}

fn write_package(
    path: &Path,
    catalog: &AssetCatalog,
    cooked: &BTreeMap<AssetId, CookedAsset>,
) -> anyhow::Result<()> {
    let mut catalog_bytes = serde_json::to_vec(catalog)?;
    catalog_bytes.push(b'\n');
    let objects = unique_objects(catalog, cooked)?;

    let mut writer = configured_writer()?;
    let directory = NodeHeader::new(DIR_MODE, 0, 0, 0);
    let file = NodeHeader::new(FILE_MODE, 0, 0, 0);
    writer.push_dir("blackflower", directory)?;
    writer.push_dir("objects", directory)?;
    writer.push_dir("objects/blake3", directory)?;
    writer.push_file(Cursor::new(catalog_bytes), "blackflower/catalog.json", file)?;
    for (content_hash, bytes) in objects {
        writer.push_file(
            Cursor::new(bytes),
            format!("objects/blake3/{content_hash}"),
            file,
        )?;
    }
    let mut output =
        File::create(path).with_context(|| format!("failed to create `{}`", path.display()))?;
    let _written = writer.write(&mut output)?;
    output
        .sync_all()
        .with_context(|| format!("failed to sync `{}`", path.display()))?;
    Ok(())
}

fn configured_writer() -> anyhow::Result<FilesystemWriter<'static, 'static, 'static>> {
    let mut writer = FilesystemWriter::default();
    writer.set_block_size(BLOCK_SIZE);
    writer.set_time(0);
    writer.set_only_root_id();
    writer.set_root_mode(DIR_MODE);
    writer.set_root_uid(0);
    writer.set_root_gid(0);
    writer.set_kib_padding(4);
    writer.set_no_duplicate_files(true);
    writer.set_emit_compression_options(true);
    let compressor = FilesystemCompressor::new(
        Compressor::Zstd,
        Some(CompressionOptions::Zstd(Zstd {
            compression_level: ZSTD_LEVEL,
        })),
    )?;
    writer.set_compressor(compressor);
    Ok(writer)
}

fn unique_objects(
    catalog: &AssetCatalog,
    cooked: &BTreeMap<AssetId, CookedAsset>,
) -> anyhow::Result<BTreeMap<ContentHash, Bytes>> {
    let mut objects = BTreeMap::new();
    for record in &catalog.assets {
        let source = cooked
            .get(&record.id)
            .with_context(|| format!("missing object source for `{}`", record.id))?;
        objects
            .entry(record.content_hash)
            .or_insert_with(|| source.bytes.clone());
    }
    Ok(objects)
}

fn replace_output(candidate: &Path, output: &Path, staging: &Path) -> anyhow::Result<()> {
    atomic_replace(candidate, output)
        .with_context(|| format!("failed to publish `{}`", output.display()))?;
    sync_directory(staging)
}

#[cfg(not(windows))]
fn atomic_replace(candidate: &Path, output: &Path) -> std::io::Result<()> {
    fs::rename(candidate, output)
}

#[cfg(windows)]
#[allow(
    unsafe_code,
    reason = "Windows exposes atomic replacement only through its system API"
)]
fn atomic_replace(candidate: &Path, output: &Path) -> std::io::Result<()> {
    use std::os::windows::ffi::OsStrExt;

    use windows_sys::Win32::Storage::FileSystem::{
        MOVEFILE_WRITE_THROUGH, MoveFileExW, REPLACEFILE_IGNORE_MERGE_ERRORS, ReplaceFileW,
    };

    let output_exists = output.exists();
    let candidate_wide = candidate
        .as_os_str()
        .encode_wide()
        .chain(core::iter::once(0))
        .collect::<Vec<_>>();
    let output_wide = output
        .as_os_str()
        .encode_wide()
        .chain(core::iter::once(0))
        .collect::<Vec<_>>();
    let result = if output_exists {
        // SAFETY: both path pointers reference live, NUL-terminated UTF-16
        // buffers for the duration of the call; the optional pointers are null.
        unsafe {
            ReplaceFileW(
                output_wide.as_ptr(),
                candidate_wide.as_ptr(),
                core::ptr::null(),
                REPLACEFILE_IGNORE_MERGE_ERRORS,
                core::ptr::null(),
                core::ptr::null(),
            )
        }
    } else {
        // SAFETY: both pointers reference live, NUL-terminated UTF-16 buffers
        // for the duration of the call, and the flag is valid for `MoveFileExW`.
        unsafe {
            MoveFileExW(
                candidate_wide.as_ptr(),
                output_wide.as_ptr(),
                MOVEFILE_WRITE_THROUGH,
            )
        }
    };
    if result == 0 {
        Err(std::io::Error::last_os_error())
    } else {
        Ok(())
    }
}

#[cfg(unix)]
fn sync_directory(staging: &Path) -> anyhow::Result<()> {
    let directory = staging
        .parent()
        .context("staging directory has no package parent")?;
    File::open(directory)?
        .sync_all()
        .with_context(|| format!("failed to sync `{}`", directory.display()))
}

#[cfg(not(unix))]
fn sync_directory(_staging: &Path) -> anyhow::Result<()> {
    Ok(())
}

fn write_cache_object(path: &Path, bytes: &[u8], expected: ContentHash) -> anyhow::Result<()> {
    if path.is_file() {
        let existing = fs::read(path)
            .with_context(|| format!("failed to read cache object `{}`", path.display()))?;
        if ContentHash::hash_bytes(&existing) == expected {
            return Ok(());
        }
    }
    write_atomic(path, bytes)
}

fn write_atomic(path: &Path, bytes: &[u8]) -> anyhow::Result<()> {
    let parent = path.parent().context("atomic output path has no parent")?;
    fs::create_dir_all(parent)?;
    let mut temporary = NamedTempFile::new_in(parent)?;
    temporary.write_all(bytes)?;
    temporary.as_file().sync_all()?;
    temporary
        .persist(path)
        .map_err(|error| error.error)
        .with_context(|| format!("failed to publish `{}`", path.display()))?;
    Ok(())
}

#[derive(Debug, Serialize)]
struct CachedRecipe<'a> {
    schema: u32,
    id: &'a AssetId,
    content_hash: ContentHash,
    recipe_hash: RecipeHash,
}

#[cfg(test)]
mod tests {
    use std::collections::{BTreeMap, BTreeSet};
    use std::fs;
    use std::str::FromStr;
    use std::sync::Arc;
    use std::sync::mpsc::RecvTimeoutError;
    use std::time::{Duration, Instant};

    use anyhow::Context;
    use blackflower_animation_format::{AnimationContainer, SkeletonContainer};
    use blackflower_assets::{
        AssetCatalog, AssetChangeKind, AssetId, AssetKind, AssetPackage, AssetReloadStatus,
        AssetSigningKey, AssetStore, AssetStoreManager, AssetStoreWatcher, AssetTrustStore,
        AssetWatchEvent, Bytes, ContentHash, Error, PackageName, ProfileName, sign_package,
    };
    use blackflower_navigation::NavMeshAsset;
    use blackflower_rendering_models::MeshAsset;
    use blackflower_scripting::{Bytecode, Runtime, Value};
    use tempfile::TempDir;

    use crate::asset_cooker::{CookedAsset, cook_assets};
    use crate::manifest::Repository;
    use crate::profile::CookingProfiles;

    use super::{CookRequest, Pipeline, build_catalog, toolchain_identity, write_package};

    const TEST_SIGNING_SECRET: [u8; 32] = [0x42; 32];
    const DEBUG_PROFILE: &str = r#"schema = 1

[scripting.luau]
optimization = "baseline"
debug = "full"
type_info = "native_modules"

[shaders]
target = "spirv"
capability = "spirv_1_5"
optimization = "none"
debug = "standard"

[textures]
ldr_encoding = "uastc"
hdr_encoding = "rgba16f"
quality = "fast"
zstd_level = 3
generate_mipmaps = true

[meshes]
lod_triangle_percents = [50, 25, 12]
lod_target_error = 0.01
optimize_overdraw = true
overdraw_threshold = 1.05
lock_borders = true

[animations]
sampling_rate_hz = 0.0
iframe_interval_seconds = 10.0
optimize = true
optimization_tolerance = 0.001
optimization_distance = 0.1
root_motion_tolerance = 0.001
"#;
    const TEXTURE_RELEASE_PROFILE: &str = r#"schema = 1

[scripting.luau]
optimization = "baseline"
debug = "full"
type_info = "native_modules"

[shaders]
target = "spirv"
capability = "spirv_1_5"
optimization = "none"
debug = "standard"

[textures]
ldr_encoding = "uastc"
hdr_encoding = "rgba16f"
quality = "high"
zstd_level = 15
generate_mipmaps = true

[meshes]
lod_triangle_percents = [50, 25, 12]
lod_target_error = 0.01
optimize_overdraw = true
overdraw_threshold = 1.05
lock_borders = true

[animations]
sampling_rate_hz = 0.0
iframe_interval_seconds = 10.0
optimize = true
optimization_tolerance = 0.001
optimization_distance = 0.1
root_motion_tolerance = 0.001
"#;
    const RELEASE_PROFILE: &str = r#"schema = 1

[scripting.luau]
optimization = "aggressive"
debug = "line_info"
type_info = "native_modules"

[shaders]
target = "spirv"
capability = "spirv_1_5"
optimization = "high"
debug = "none"

[textures]
ldr_encoding = "uastc"
hdr_encoding = "rgba16f"
quality = "high"
zstd_level = 15
generate_mipmaps = true

[meshes]
lod_triangle_percents = [50, 25, 12]
lod_target_error = 0.01
optimize_overdraw = true
overdraw_threshold = 1.05
lock_borders = true

[animations]
sampling_rate_hz = 0.0
iframe_interval_seconds = 10.0
optimize = true
optimization_tolerance = 0.001
optimization_distance = 0.1
root_motion_tolerance = 0.001
"#;
    const LUAU_RELEASE_PROFILE: &str = r#"schema = 1

[scripting.luau]
optimization = "aggressive"
debug = "line_info"
type_info = "native_modules"

[shaders]
target = "spirv"
capability = "spirv_1_5"
optimization = "none"
debug = "standard"

[textures]
ldr_encoding = "uastc"
hdr_encoding = "rgba16f"
quality = "fast"
zstd_level = 3
generate_mipmaps = true

[meshes]
lod_triangle_percents = [50, 25, 12]
lod_target_error = 0.01
optimize_overdraw = true
overdraw_threshold = 1.05
lock_borders = true

[animations]
sampling_rate_hz = 0.0
iframe_interval_seconds = 10.0
optimize = true
optimization_tolerance = 0.001
optimization_distance = 0.1
root_motion_tolerance = 0.001
"#;
    const SHADER_RELEASE_PROFILE: &str = r#"schema = 1

[scripting.luau]
optimization = "baseline"
debug = "full"
type_info = "native_modules"

[shaders]
target = "spirv"
capability = "spirv_1_5"
optimization = "high"
debug = "none"

[textures]
ldr_encoding = "uastc"
hdr_encoding = "rgba16f"
quality = "fast"
zstd_level = 3
generate_mipmaps = true

[meshes]
lod_triangle_percents = [50, 25, 12]
lod_target_error = 0.01
optimize_overdraw = true
overdraw_threshold = 1.05
lock_borders = true

[animations]
sampling_rate_hz = 0.0
iframe_interval_seconds = 10.0
optimize = true
optimization_tolerance = 0.001
optimization_distance = 0.1
root_motion_tolerance = 0.001
"#;

    #[test]
    fn cooks_deterministically_and_reuses_the_logical_name() -> anyhow::Result<()> {
        let fixture = Fixture::new()?;
        fixture.asset("fixtures/example", "shared", "example.bin", b"first")?;
        let request = fixture.request("pak000", &["fixtures/example"])?;
        let first = fixture.pipeline.cook(&request)?;
        let first_bytes = fs::read(&first.path)?;
        let second = fixture.pipeline.cook(&request)?;
        assert_eq!(first.package_hash, second.package_hash);
        assert_eq!(first_bytes, fs::read(&second.path)?);
        Ok(())
    }

    #[test]
    fn cooks_profile_configured_luau_bytecode_for_runtime_loading() -> anyhow::Result<()> {
        let fixture = Fixture::new()?;
        fixture.luau(
            "scripts/answer",
            "simulation",
            "answer.luau",
            b"local function answer() return 42 end\nreturn answer()\n",
        )?;
        let request = fixture.request("pak000", &["scripts/answer"])?;
        let _result = fixture.pipeline.cook(&request)?;

        let store = fixture.open_store()?;
        let id = AssetId::from_str("scripts/answer")?;
        let resolved = store.resolve(&id).context("missing cooked Luau asset")?;
        assert_eq!(
            resolved.record().kind,
            blackflower_assets::AssetKind::LuauBytecode
        );
        assert_eq!(resolved.package().catalog().profile.name.as_str(), "debug");
        assert_eq!(resolved.package().catalog().toolchain.luau, "luau/0.731.0");

        let bytes = store.read_asset(&id)?;
        assert_ne!(
            bytes.as_ref(),
            b"local function answer() return 42 end\nreturn answer()\n"
        );
        let bytecode = Bytecode::from_bytes(bytes.to_vec());
        let values = Runtime::new()?.execute_bytecode("scripts/answer", &bytecode)?;
        assert!(matches!(
            values.as_slice(),
            [Value::Number(value)] if value.to_bits() == 42.0_f64.to_bits()
        ));
        Ok(())
    }

    #[test]
    fn cooks_profile_configured_shader_module_for_runtime_loading() -> anyhow::Result<()> {
        let fixture = Fixture::new()?;
        fixture.shader(
            "shaders/basic",
            "presentation",
            "basic.slang",
            b"float4 vertex_main(float4 position : POSITION) : SV_Position\n{\n    return position;\n}\n",
            "vertex_main",
            "vertex",
        )?;
        let request = fixture.request("pak000", &["shaders/basic"])?;
        let _result = fixture.pipeline.cook(&request)?;

        let store = fixture.open_store()?;
        let id = AssetId::from_str("shaders/basic")?;
        let resolved = store.resolve(&id).context("missing cooked shader asset")?;
        assert_eq!(
            resolved.record().kind,
            blackflower_assets::AssetKind::ShaderModule
        );
        assert_eq!(
            resolved.package().catalog().toolchain.slang,
            "slang/2026.14.1"
        );
        assert_eq!(resolved.package().catalog().toolchain.naga, "naga/30.0.0");

        let bytes = store.read_asset(&id)?;
        assert_eq!(&bytes[..4], &0x0723_0203_u32.to_le_bytes());
        Ok(())
    }

    #[test]
    fn cooks_texture_for_runtime_capability_selection() -> anyhow::Result<()> {
        let fixture = Fixture::new()?;
        let png = png_texture()?;
        fixture.texture(
            "textures/checker",
            "presentation",
            "checker.png",
            &png,
            "color_srgb",
        )?;
        let request = fixture.request("pak000", &["textures/checker"])?;
        let _result = fixture.pipeline.cook(&request)?;

        let store = fixture.open_store()?;
        let id = AssetId::from_str("textures/checker")?;
        let resolved = store.resolve(&id).context("missing cooked texture")?;
        assert_eq!(
            resolved.record().kind,
            blackflower_assets::AssetKind::Texture2d
        );
        assert_eq!(resolved.package().catalog().toolchain.ktx, "ktx/4.4.2");
        assert_eq!(
            resolved.package().catalog().toolchain.texture_decoder,
            "image/0.25.10+half/2.7.1"
        );

        let texture =
            blackflower_rendering_textures::TextureAsset::from_bytes(store.read_asset(&id)?)?;
        assert_eq!(texture.dimensions(), (3, 2));
        assert_eq!(texture.level_count(), 2);
        assert_eq!(
            texture.semantic(),
            blackflower_rendering_textures::TextureSemantic::ColorSrgb
        );
        let upload = texture
            .transcode(blackflower_rendering_textures::TextureTargetCapabilities::default())?;
        assert_eq!(
            upload.format,
            blackflower_rendering_textures::TextureFormat::Rgba8
        );
        assert_eq!(upload.levels.len(), 2);
        Ok(())
    }

    #[test]
    fn cooks_hdr_texture_without_lossy_basis_transcoding() -> anyhow::Result<()> {
        let fixture = Fixture::new()?;
        fixture.texture(
            "textures/sky",
            "presentation",
            "sky.exr",
            &exr_texture()?,
            "hdr_linear",
        )?;
        let request = fixture.request("pak000", &["textures/sky"])?;
        let _result = fixture.pipeline.cook(&request)?;

        let store = fixture.open_store()?;
        let id = AssetId::from_str("textures/sky")?;
        let texture =
            blackflower_rendering_textures::TextureAsset::from_bytes(store.read_asset(&id)?)?;
        assert_eq!(texture.dimensions(), (2, 2));
        assert_eq!(
            texture.semantic(),
            blackflower_rendering_textures::TextureSemantic::HdrLinear
        );
        let upload = texture
            .transcode(blackflower_rendering_textures::TextureTargetCapabilities::default())?;
        assert_eq!(
            upload.format,
            blackflower_rendering_textures::TextureFormat::Rgba16Float
        );
        assert_eq!(upload.levels.len(), 2);
        Ok(())
    }

    #[test]
    fn cooks_optimized_static_mesh_with_generated_lods() -> anyhow::Result<()> {
        let fixture = Fixture::new()?;
        let (gltf, buffer) = grid_gltf(9)?;
        fixture.mesh(
            "models/grid",
            "presentation",
            "grid.gltf",
            &gltf,
            "grid.bin",
            &buffer,
            "Grid",
        )?;
        let request = fixture.request("pak000", &["models/grid"])?;
        let first = fixture.pipeline.cook(&request)?;
        let first_package = fs::read(&first.path)?;

        let store = fixture.open_store()?;
        let id = AssetId::from_str("models/grid")?;
        let resolved = store.resolve(&id).context("missing cooked mesh")?;
        assert_eq!(resolved.record().kind, blackflower_assets::AssetKind::Mesh);
        assert_eq!(
            resolved.package().catalog().toolchain.meshoptimizer,
            "meshopt/0.6.2"
        );
        let model = MeshAsset::from_bytes(store.read_asset(&id)?)?;
        assert_eq!(model.primitives().len(), 1);
        let lods = model.primitives()[0].lods();
        assert!(lods.len() >= 2);
        assert!(
            lods.windows(2)
                .all(|pair| pair[1].indices().len() < pair[0].indices().len())
        );

        drop(store);
        let second = fixture.pipeline.cook(&request)?;
        assert_eq!(first.package_hash, second.package_hash);
        assert_eq!(first_package, fs::read(second.path)?);
        Ok(())
    }

    #[test]
    #[allow(
        clippy::too_many_lines,
        reason = "the end-to-end package proof keeps the explicit navigation manifest and runtime assertions together"
    )]
    fn cooks_navigation_manifest_to_runtime_bfnav() -> anyhow::Result<()> {
        let fixture = Fixture::new()?;
        let (gltf, buffer) = navigation_floor_gltf()?;
        let directory = fixture.source.join("levels/arena");
        fs::create_dir_all(&directory)?;
        fs::write(directory.join("navigation.gltf"), gltf)?;
        fs::write(directory.join("navigation.bin"), buffer)?;
        fs::write(
            directory.join("asset.toml"),
            r#"schema = 1
id = "levels/arena/navigation/humanoid"
kind = "navigation_mesh"
audience = "simulation"

[navigation]
source = "navigation.gltf"
profile_id = "humanoid"

[navigation.agent]
height = 1.8
radius = 0.35
max_climb = 0.4
max_slope_degrees = 45.0

[navigation.build]
cell_size = 0.2
cell_height = 0.1
tile_size = 64
region_min_area = 1
region_merge_area = 1
max_edge_length = 12.0
max_simplification_error = 1.3
max_vertices_per_polygon = 6
detail_sample_distance = 6.0
detail_sample_max_error = 1.0

[[navigation.areas]]
key = "ground"
traversable = true
cost = 1.0

[[navigation.areas]]
key = "water"
traversable = false
"#,
        )?;
        let request = fixture.request("pak000", &["levels/arena/navigation/humanoid"])?;
        let first = fixture.pipeline.cook(&request)?;
        let first_package = fs::read(&first.path)?;

        let store = fixture.open_store()?;
        let id = AssetId::from_str("levels/arena/navigation/humanoid")?;
        let resolved = store.resolve(&id).context("missing navigation asset")?;
        assert_eq!(resolved.record().kind, AssetKind::NavigationMesh);
        assert!(resolved.record().dependencies.is_empty());
        assert!(
            resolved
                .package()
                .catalog()
                .toolchain
                .recast_navigation
                .starts_with("recastnavigation/1.6.0@")
        );
        let asset = NavMeshAsset::from_bytes(store.read_asset(&id)?)?;
        assert_eq!(asset.agent().id().as_str(), "humanoid");
        assert_eq!(asset.areas()[0].key().as_str(), "ground");
        assert_eq!(asset.areas()[1].key().as_str(), "water");
        let _navmesh = asset.instantiate()?;

        drop(store);
        let second = fixture.pipeline.cook(&request)?;
        assert_eq!(first.package_hash, second.package_hash);
        assert_eq!(first_package, fs::read(second.path)?);
        Ok(())
    }

    #[test]
    #[allow(
        clippy::too_many_lines,
        reason = "the package-level end-to-end proof keeps shared-source setup and catalog assertions together"
    )]
    fn animation_clip_selects_and_cooks_its_skeleton_dependency() -> anyhow::Result<()> {
        let fixture = Fixture::new()?;
        let source = rigged_animation_gltf()?;
        let directory = fixture.source.join("characters/hero");
        fs::create_dir_all(&directory)?;
        fs::write(directory.join("rig.gltf"), source)?;
        fs::write(
            directory.join("rig.asset.toml"),
            "schema = 1\nid = \"characters/rig\"\nkind = \"skeleton\"\naudience = \"presentation\"\n\n[skeleton]\nsource = \"rig.gltf\"\nskin = \"Armature\"\n",
        )?;
        fs::write(
            directory.join("walk.asset.toml"),
            "schema = 1\nid = \"characters/walk\"\nkind = \"animation_clip\"\naudience = \"presentation\"\n\n[animation]\nsource = \"rig.gltf\"\nclip = \"Walk\"\nskeleton = \"characters/rig\"\n",
        )?;
        fs::write(
            directory.join("lean.asset.toml"),
            "schema = 1\nid = \"characters/lean\"\nkind = \"animation_clip\"\naudience = \"presentation\"\n\n[animation]\nsource = \"rig.gltf\"\nclip = \"Lean\"\nskeleton = \"characters/rig\"\n",
        )?;
        let request = fixture.request("pak000", &["characters/walk", "characters/lean"])?;
        let first = fixture.pipeline.cook(&request)?;
        let first_package = fs::read(&first.path)?;

        let store = fixture.open_store()?;
        let skeleton_id = AssetId::from_str("characters/rig")?;
        let animation_id = AssetId::from_str("characters/walk")?;
        let additive_id = AssetId::from_str("characters/lean")?;
        let skeleton_record = store
            .resolve(&skeleton_id)
            .context("missing automatic skeleton dependency")?
            .record();
        let animation_record = store
            .resolve(&animation_id)
            .context("missing cooked animation clip")?
            .record();
        assert_eq!(skeleton_record.kind, AssetKind::Skeleton);
        assert_eq!(animation_record.kind, AssetKind::AnimationClip);
        assert_eq!(
            animation_record.dependencies.as_slice(),
            std::slice::from_ref(&skeleton_id)
        );
        let additive_record = store
            .resolve(&additive_id)
            .context("missing second clip from shared glTF")?
            .record();
        assert_eq!(additive_record.kind, AssetKind::AnimationClip);
        assert_eq!(
            additive_record.dependencies.as_slice(),
            std::slice::from_ref(&skeleton_id)
        );

        let skeleton_bytes = store.read_asset(&skeleton_id)?;
        let animation_bytes = store.read_asset(&animation_id)?;
        let additive_bytes = store.read_asset(&additive_id)?;
        let skeleton = SkeletonContainer::decode(&skeleton_bytes)?;
        let animation = AnimationContainer::decode(&animation_bytes)?;
        let additive = AnimationContainer::decode(&additive_bytes)?;
        assert_eq!(animation.skeleton_identity(), skeleton.identity());
        assert_eq!(additive.skeleton_identity(), skeleton.identity());
        assert!(animation.metadata().looping());
        assert!(animation.ozz_root_motion().is_some());
        assert!(additive.metadata().additive());
        assert!(additive.ozz_root_motion().is_none());
        let markers = animation.metadata().markers();
        assert_eq!(markers.len(), 3);
        assert_eq!(markers[0].name(), "start");
        assert_eq!(markers[0].ratio().to_bits(), 0.0_f32.to_bits());
        assert_eq!(markers[2].name(), "end");
        assert_eq!(markers[2].ratio().to_bits(), 1.0_f32.to_bits());
        drop(store);

        let second = fixture.pipeline.cook(&request)?;
        assert_eq!(first.package_hash, second.package_hash);
        assert_eq!(first_package, fs::read(second.path)?);
        Ok(())
    }

    #[test]
    fn external_gltf_buffers_participate_in_mesh_recipe_identity() -> anyhow::Result<()> {
        let fixture = Fixture::new()?;
        let (gltf, mut buffer) = grid_gltf(9)?;
        fixture.mesh(
            "models/grid",
            "presentation",
            "grid.gltf",
            &gltf,
            "grid.bin",
            &buffer,
            "Grid",
        )?;
        let request = fixture.request("pak000", &["models/grid"])?;
        let _first_result = fixture.pipeline.cook(&request)?;
        let id = AssetId::from_str("models/grid")?;
        let first = fixture
            .open_store()?
            .resolve(&id)
            .context("missing first mesh")?
            .record()
            .clone();

        let position_byte = buffer
            .first_mut()
            .context("grid buffer unexpectedly empty")?;
        *position_byte ^= 1;
        fs::write(fixture.source.join("models/grid/grid.bin"), buffer)?;
        let _second_result = fixture.pipeline.cook(&request)?;
        let second = fixture
            .open_store()?
            .resolve(&id)
            .context("missing second mesh")?
            .record()
            .clone();
        assert_ne!(first.recipe_hash, second.recipe_hash);
        assert_ne!(first.content_hash, second.content_hash);
        Ok(())
    }

    #[test]
    fn rejects_invalid_mesh_manifest_and_selection_before_publication() -> anyhow::Result<()> {
        let wrong_audience = Fixture::new()?;
        let (gltf, buffer) = grid_gltf(5)?;
        wrong_audience.mesh(
            "models/grid",
            "simulation",
            "grid.gltf",
            &gltf,
            "grid.bin",
            &buffer,
            "Grid",
        )?;
        let error = wrong_audience
            .pipeline
            .check()
            .err()
            .context("expected mesh audience rejection")?;
        assert!(format!("{error:#}").contains("must use audience `presentation`"));

        let missing_mesh = Fixture::new()?;
        missing_mesh.mesh(
            "models/grid",
            "presentation",
            "grid.gltf",
            &gltf,
            "grid.bin",
            &buffer,
            "Missing",
        )?;
        let request = missing_mesh.request("pak000", &["models/grid"])?;
        let error = missing_mesh
            .pipeline
            .cook(&request)
            .err()
            .context("expected missing named mesh rejection")?;
        assert!(format!("{error:#}").contains("contains no mesh named `Missing`"));
        assert!(!missing_mesh.package_dir().join("pak000.squashfs").exists());
        Ok(())
    }

    #[test]
    fn rejects_empty_shader_toolchain_identities() -> anyhow::Result<()> {
        let fixture = Fixture::new()?;
        fixture.asset("fixtures/example", "shared", "example.bin", b"bytes")?;
        let mut catalog = fixture.selected_catalog(&["fixtures/example"])?;

        let slang = catalog.toolchain.slang.clone();
        catalog.toolchain.slang.clear();
        let _path = fixture.write_catalog_package("pak000", &catalog)?;
        let error = fixture
            .open_store()
            .err()
            .context("expected empty Slang identity rejection")?;
        assert!(matches!(error, Error::InvalidCatalog { .. }));

        catalog.toolchain.slang = slang;
        catalog.toolchain.naga.clear();
        let _path = fixture.write_catalog_package("pak000", &catalog)?;
        let error = fixture
            .open_store()
            .err()
            .context("expected empty Naga identity rejection")?;
        assert!(matches!(error, Error::InvalidCatalog { .. }));
        Ok(())
    }

    #[test]
    fn rejects_empty_texture_toolchain_identities() -> anyhow::Result<()> {
        let fixture = Fixture::new()?;
        fixture.asset("fixtures/example", "shared", "example.bin", b"bytes")?;
        let mut catalog = fixture.selected_catalog(&["fixtures/example"])?;

        let ktx = catalog.toolchain.ktx.clone();
        catalog.toolchain.ktx.clear();
        let _path = fixture.write_catalog_package("pak000", &catalog)?;
        let error = fixture
            .open_store()
            .err()
            .context("expected empty KTX identity rejection")?;
        assert!(matches!(error, Error::InvalidCatalog { .. }));

        catalog.toolchain.ktx = ktx;
        let decoder = catalog.toolchain.texture_decoder.clone();
        catalog.toolchain.texture_decoder.clear();
        let _path = fixture.write_catalog_package("pak000", &catalog)?;
        let error = fixture
            .open_store()
            .err()
            .context("expected empty texture decoder identity rejection")?;
        assert!(matches!(error, Error::InvalidCatalog { .. }));

        catalog.toolchain.texture_decoder = decoder;
        catalog.toolchain.texture_encoder_platform.clear();
        let _path = fixture.write_catalog_package("pak000", &catalog)?;
        let error = fixture
            .open_store()
            .err()
            .context("expected empty texture platform identity rejection")?;
        assert!(matches!(error, Error::InvalidCatalog { .. }));
        Ok(())
    }

    #[test]
    fn rejects_empty_meshoptimizer_toolchain_identity() -> anyhow::Result<()> {
        let fixture = Fixture::new()?;
        fixture.asset("fixtures/example", "shared", "example.bin", b"bytes")?;
        let mut catalog = fixture.selected_catalog(&["fixtures/example"])?;
        let meshoptimizer = catalog.toolchain.meshoptimizer.clone();
        catalog.toolchain.meshoptimizer.clear();
        let _path = fixture.write_catalog_package("pak000", &catalog)?;
        let error = fixture
            .open_store()
            .err()
            .context("expected empty meshoptimizer identity rejection")?;
        assert!(matches!(error, Error::InvalidCatalog { .. }));

        catalog.toolchain.meshoptimizer = meshoptimizer;
        catalog.toolchain.ozz_animation.clear();
        let _path = fixture.write_catalog_package("pak000", &catalog)?;
        let error = fixture
            .open_store()
            .err()
            .context("expected empty ozz-animation identity rejection")?;
        assert!(matches!(error, Error::InvalidCatalog { .. }));
        Ok(())
    }

    #[test]
    fn luau_profile_settings_change_only_luau_recipes() -> anyhow::Result<()> {
        let fixture = Fixture::new()?;
        fixture.asset("fixtures/blob", "shared", "blob.bin", b"blob")?;
        fixture.luau(
            "scripts/policy",
            "simulation",
            "policy.luau",
            b"local function policy(value) return value + 1 end\nreturn policy(4)\n",
        )?;
        let request = fixture.request("pak000", &["fixtures/blob", "scripts/policy"])?;
        let first_result = fixture.pipeline.cook(&request)?;
        let first = fixture.open_store()?;
        let blob = AssetId::from_str("fixtures/blob")?;
        let luau = AssetId::from_str("scripts/policy")?;
        let first_blob = first
            .resolve(&blob)
            .context("missing blob")?
            .record()
            .clone();
        let first_luau = first
            .resolve(&luau)
            .context("missing Luau")?
            .record()
            .clone();
        let first_profile = first.packages()[0].catalog().profile.clone();
        drop(first);

        fixture.write_profile("debug", LUAU_RELEASE_PROFILE)?;
        let second_result = fixture.pipeline.cook(&request)?;
        let second = fixture.open_store()?;
        let second_blob = second
            .resolve(&blob)
            .context("missing recooked blob")?
            .record();
        let second_luau = second
            .resolve(&luau)
            .context("missing recooked Luau")?
            .record();
        let second_profile = &second.packages()[0].catalog().profile;

        assert_eq!(first_blob.content_hash, second_blob.content_hash);
        assert_eq!(first_blob.recipe_hash, second_blob.recipe_hash);
        assert_ne!(first_luau.recipe_hash, second_luau.recipe_hash);
        assert_ne!(first_profile.hash, second_profile.hash);
        assert_ne!(first_result.package_hash, second_result.package_hash);
        Ok(())
    }

    #[test]
    fn shader_profile_settings_change_only_shader_recipes() -> anyhow::Result<()> {
        let fixture = Fixture::new()?;
        fixture.asset("fixtures/blob", "shared", "blob.bin", b"blob")?;
        fixture.shader(
            "shaders/basic",
            "presentation",
            "basic.slang",
            b"float4 vertex_main(float4 position : POSITION) : SV_Position\n{\n    return position;\n}\n",
            "vertex_main",
            "vertex",
        )?;
        let request = fixture.request("pak000", &["fixtures/blob", "shaders/basic"])?;
        let first_result = fixture.pipeline.cook(&request)?;
        let first = fixture.open_store()?;
        let blob = AssetId::from_str("fixtures/blob")?;
        let shader = AssetId::from_str("shaders/basic")?;
        let first_blob = first
            .resolve(&blob)
            .context("missing blob")?
            .record()
            .clone();
        let first_shader = first
            .resolve(&shader)
            .context("missing shader")?
            .record()
            .clone();
        let first_profile = first.packages()[0].catalog().profile.clone();
        drop(first);

        fixture.write_profile("debug", SHADER_RELEASE_PROFILE)?;
        let second_result = fixture.pipeline.cook(&request)?;
        let second = fixture.open_store()?;
        let second_blob = second
            .resolve(&blob)
            .context("missing recooked blob")?
            .record();
        let second_shader = second
            .resolve(&shader)
            .context("missing recooked shader")?
            .record();
        let second_profile = &second.packages()[0].catalog().profile;

        assert_eq!(first_blob.content_hash, second_blob.content_hash);
        assert_eq!(first_blob.recipe_hash, second_blob.recipe_hash);
        assert_ne!(first_shader.recipe_hash, second_shader.recipe_hash);
        assert_ne!(first_profile.hash, second_profile.hash);
        assert_ne!(first_result.package_hash, second_result.package_hash);
        Ok(())
    }

    #[test]
    fn texture_profile_settings_change_only_texture_recipes() -> anyhow::Result<()> {
        let fixture = Fixture::new()?;
        fixture.asset("fixtures/blob", "shared", "blob.bin", b"blob")?;
        fixture.texture(
            "textures/checker",
            "presentation",
            "checker.png",
            &png_texture()?,
            "color_srgb",
        )?;
        let request = fixture.request("pak000", &["fixtures/blob", "textures/checker"])?;
        let first_result = fixture.pipeline.cook(&request)?;
        let first = fixture.open_store()?;
        let blob = AssetId::from_str("fixtures/blob")?;
        let texture = AssetId::from_str("textures/checker")?;
        let first_blob = first
            .resolve(&blob)
            .context("missing blob")?
            .record()
            .clone();
        let first_texture = first
            .resolve(&texture)
            .context("missing texture")?
            .record()
            .clone();
        let first_profile = first.packages()[0].catalog().profile.clone();
        drop(first);

        fixture.write_profile("debug", TEXTURE_RELEASE_PROFILE)?;
        let second_result = fixture.pipeline.cook(&request)?;
        let second = fixture.open_store()?;
        let second_blob = second
            .resolve(&blob)
            .context("missing recooked blob")?
            .record();
        let second_texture = second
            .resolve(&texture)
            .context("missing recooked texture")?
            .record();
        let second_profile = &second.packages()[0].catalog().profile;

        assert_eq!(first_blob.content_hash, second_blob.content_hash);
        assert_eq!(first_blob.recipe_hash, second_blob.recipe_hash);
        assert_ne!(first_texture.recipe_hash, second_texture.recipe_hash);
        assert_ne!(first_profile.hash, second_profile.hash);
        assert_ne!(first_result.package_hash, second_result.package_hash);
        Ok(())
    }

    #[test]
    fn rejects_unknown_or_malformed_cooking_profile_settings() -> anyhow::Result<()> {
        let unknown = Fixture::new()?;
        unknown.write_profile(
            "debug",
            "schema = 1\nunknown = true\n\n[scripting.luau]\noptimization = \"aggressive\"\ndebug = \"none\"\ntype_info = \"native_modules\"\n\n[shaders]\ntarget = \"spirv\"\ncapability = \"spirv_1_5\"\noptimization = \"high\"\ndebug = \"none\"\n",
        )?;
        assert!(unknown.pipeline.check().is_err());

        let invalid_value = Fixture::new()?;
        invalid_value.write_profile(
            "debug",
            "schema = 1\n\n[scripting.luau]\noptimization = \"fastest\"\ndebug = \"none\"\ntype_info = \"native_modules\"\n\n[shaders]\ntarget = \"spirv\"\ncapability = \"spirv_1_5\"\noptimization = \"high\"\ndebug = \"none\"\n",
        )?;
        assert!(invalid_value.pipeline.check().is_err());

        let removed_coverage = Fixture::new()?;
        removed_coverage.write_profile(
            "debug",
            "schema = 1\n\n[scripting.luau]\noptimization = \"aggressive\"\ndebug = \"none\"\ntype_info = \"native_modules\"\ncoverage = \"none\"\n\n[shaders]\ntarget = \"spirv\"\ncapability = \"spirv_1_5\"\noptimization = \"high\"\ndebug = \"none\"\n",
        )?;
        assert!(removed_coverage.pipeline.check().is_err());

        let invalid_shader_target = Fixture::new()?;
        invalid_shader_target.write_profile(
            "debug",
            "schema = 1\n\n[scripting.luau]\noptimization = \"aggressive\"\ndebug = \"none\"\ntype_info = \"native_modules\"\n\n[shaders]\ntarget = \"metal\"\ncapability = \"spirv_1_5\"\noptimization = \"high\"\ndebug = \"none\"\n",
        )?;
        assert!(invalid_shader_target.pipeline.check().is_err());

        let invalid_texture_settings = Fixture::new()?;
        invalid_texture_settings.write_profile(
            "debug",
            "schema = 1\n\n[scripting.luau]\noptimization = \"baseline\"\ndebug = \"full\"\ntype_info = \"native_modules\"\n\n[shaders]\ntarget = \"spirv\"\ncapability = \"spirv_1_5\"\noptimization = \"none\"\ndebug = \"standard\"\n\n[textures]\nldr_encoding = \"uastc\"\nhdr_encoding = \"rgba16f\"\nquality = \"fast\"\nzstd_level = 0\ngenerate_mipmaps = false\n",
        )?;
        assert!(invalid_texture_settings.pipeline.check().is_err());

        let invalid_model_settings = Fixture::new()?;
        let invalid_profile = DEBUG_PROFILE.replace(
            "lod_triangle_percents = [50, 25, 12]",
            "lod_triangle_percents = [50, 75]",
        );
        invalid_model_settings.write_profile("debug", &invalid_profile)?;
        assert!(invalid_model_settings.pipeline.check().is_err());
        Ok(())
    }

    #[test]
    fn rejects_invalid_luau_before_publishing_a_package() -> anyhow::Result<()> {
        let fixture = Fixture::new()?;
        fixture.luau("scripts/invalid", "simulation", "invalid.luau", b"local =")?;
        let request = fixture.request("pak000", &["scripts/invalid"])?;
        let error = fixture
            .pipeline
            .cook(&request)
            .err()
            .context("expected Luau compilation failure")?;
        assert!(format!("{error:#}").contains("Luau compiler rejected source"));
        assert!(!fixture.package_dir().join("pak000.squashfs").exists());
        Ok(())
    }

    #[test]
    fn rejects_invalid_shader_before_publishing_a_package() -> anyhow::Result<()> {
        let fixture = Fixture::new()?;
        fixture.shader(
            "shaders/invalid",
            "presentation",
            "invalid.slang",
            b"not valid Slang",
            "main",
            "fragment",
        )?;
        let request = fixture.request("pak000", &["shaders/invalid"])?;
        let error = fixture
            .pipeline
            .cook(&request)
            .err()
            .context("expected shader compilation failure")?;
        assert!(format!("{error:#}").contains("Slang compiler rejected source"));
        assert!(!fixture.package_dir().join("pak000.squashfs").exists());
        Ok(())
    }

    #[test]
    fn rejects_invalid_texture_before_publishing_a_package() -> anyhow::Result<()> {
        let fixture = Fixture::new()?;
        fixture.texture(
            "textures/invalid",
            "presentation",
            "invalid.png",
            b"not a PNG",
            "color_srgb",
        )?;
        let request = fixture.request("pak000", &["textures/invalid"])?;
        let error = fixture
            .pipeline
            .cook(&request)
            .err()
            .context("expected texture decoding failure")?;
        assert!(format!("{error:#}").contains("image decoder rejected texture source"));
        assert!(!fixture.package_dir().join("pak000.squashfs").exists());
        Ok(())
    }

    #[test]
    fn rejects_texture_semantic_extension_and_audience_mismatches() -> anyhow::Result<()> {
        let hdr_png = Fixture::new()?;
        hdr_png.texture(
            "textures/sky",
            "presentation",
            "sky.png",
            &png_texture()?,
            "hdr_linear",
        )?;
        let error = hdr_png
            .pipeline
            .check()
            .err()
            .context("expected HDR extension rejection")?;
        assert!(format!("{error:#}").contains("PNG for LDR semantics or EXR for HDR"));

        let simulation = Fixture::new()?;
        simulation.texture(
            "textures/checker",
            "simulation",
            "checker.png",
            &png_texture()?,
            "color_srgb",
        )?;
        let error = simulation
            .pipeline
            .check()
            .err()
            .context("expected texture audience rejection")?;
        assert!(format!("{error:#}").contains("must use audience `presentation`"));
        Ok(())
    }

    #[test]
    fn rejects_mixed_profile_hashes_in_one_package_directory() -> anyhow::Result<()> {
        let fixture = Fixture::new()?;
        fixture.asset("fixtures/example", "shared", "example.bin", b"bytes")?;
        let _base = fixture
            .pipeline
            .cook(&fixture.request("pak000", &["fixtures/example"])?)?;

        fixture.write_profile("debug", RELEASE_PROFILE)?;
        let _additional = fixture
            .pipeline
            .cook(&fixture.request("pak100", &["fixtures/example"])?)?;

        let error = fixture
            .open_store()
            .err()
            .context("expected mixed profile rejection")?;
        assert!(matches!(error, Error::IncompatibleProfile { .. }));
        Ok(())
    }

    #[test]
    fn higher_package_overrides_and_lower_package_fills_gaps() -> anyhow::Result<()> {
        let fixture = Fixture::new()?;
        fixture.asset("fixtures/changed", "shared", "changed.bin", b"base")?;
        fixture.asset("fixtures/lower-only", "shared", "lower.bin", b"lower")?;
        let base = fixture.request("pak000", &["fixtures/changed", "fixtures/lower-only"])?;
        let _base_result = fixture.pipeline.cook(&base)?;

        fixture.asset("fixtures/changed", "shared", "changed.bin", b"hotfix")?;
        let hotfix = fixture.request("pak900-hotfix", &["fixtures/changed"])?;
        let _hotfix_result = fixture.pipeline.cook(&hotfix)?;

        let store = fixture.open_store()?;
        let changed = AssetId::from_str("fixtures/changed")?;
        let lower = AssetId::from_str("fixtures/lower-only")?;
        let changed_bytes: Bytes = store.read_asset(&changed)?;
        let lower_bytes: Bytes = store.read_asset(&lower)?;
        assert_eq!(changed_bytes.as_ref(), b"hotfix");
        assert_eq!(lower_bytes.as_ref(), b"lower");
        assert_eq!(
            store
                .resolve(&changed)
                .map(|resolved| resolved.package().name().as_str()),
            Some("pak900-hotfix")
        );
        Ok(())
    }

    #[test]
    fn incompatible_audience_override_is_rejected() -> anyhow::Result<()> {
        let fixture = Fixture::new()?;
        fixture.asset("fixtures/example", "simulation", "example.bin", b"base")?;
        let _base = fixture
            .pipeline
            .cook(&fixture.request("pak000", &["fixtures/example"])?)?;
        fixture.asset(
            "fixtures/example",
            "presentation",
            "example.bin",
            b"override",
        )?;
        let _override = fixture
            .pipeline
            .cook(&fixture.request("pak900", &["fixtures/example"])?)?;
        let error = fixture.open_store().err().context("expected error")?;
        assert!(matches!(error, Error::IncompatibleOverride { .. }));
        Ok(())
    }

    #[test]
    fn renaming_a_package_changes_the_asset_set_hash() -> anyhow::Result<()> {
        let fixture = Fixture::new()?;
        fixture.asset("fixtures/example", "shared", "example.bin", b"bytes")?;
        let cooked = fixture
            .pipeline
            .cook(&fixture.request("pak000", &["fixtures/example"])?)?;
        let first = fixture.open_store()?.asset_set_hash();
        let renamed = fixture.package_dir().join("pak100.squashfs");
        fs::rename(cooked.path, renamed)?;
        let second = fixture.open_store()?.asset_set_hash();
        assert_ne!(first, second);
        Ok(())
    }

    #[test]
    fn content_and_composition_change_the_asset_set_hash() -> anyhow::Result<()> {
        let fixture = Fixture::new()?;
        fixture.asset("fixtures/example", "shared", "example.bin", b"first")?;
        let _initial = fixture
            .pipeline
            .cook(&fixture.request("pak000", &["fixtures/example"])?)?;
        let initial = fixture.open_store()?.asset_set_hash();

        fixture.asset("fixtures/example", "shared", "example.bin", b"second")?;
        let _changed = fixture
            .pipeline
            .cook(&fixture.request("pak000", &["fixtures/example"])?)?;
        let changed = fixture.open_store()?.asset_set_hash();
        assert_ne!(initial, changed);

        let _additional = fixture
            .pipeline
            .cook(&fixture.request("pak100", &["fixtures/example"])?)?;
        let composed = fixture.open_store()?.asset_set_hash();
        assert_ne!(changed, composed);
        Ok(())
    }

    #[test]
    fn hot_reload_publishes_a_diff_and_preserves_old_snapshots() -> anyhow::Result<()> {
        let fixture = Fixture::new()?;
        fixture.asset("fixtures/modified", "presentation", "modified.bin", b"old")?;
        fixture.asset("fixtures/removed", "shared", "removed.bin", b"removed")?;
        let _initial = fixture
            .pipeline
            .cook(&fixture.request("pak000", &["fixtures/modified", "fixtures/removed"])?)?;
        let manager = fixture.open_manager()?;
        let old_snapshot = manager.snapshot();

        fixture.asset("fixtures/modified", "presentation", "modified.bin", b"new")?;
        fixture.asset("fixtures/added", "simulation", "added.bin", b"added")?;
        let _changed = fixture
            .pipeline
            .cook(&fixture.request("pak000", &["fixtures/modified", "fixtures/added"])?)?;
        let reload = manager.reload()?;

        assert_eq!(reload.status(), AssetReloadStatus::Reloaded);
        assert_eq!(reload.snapshot().generation().get(), 1);
        let changes = reload.changes().changes();
        assert_eq!(changes.len(), 3);
        assert_eq!(changes[0].id().as_str(), "fixtures/added");
        assert_eq!(changes[0].change(), AssetChangeKind::Added);
        assert_eq!(changes[1].id().as_str(), "fixtures/modified");
        assert_eq!(changes[1].change(), AssetChangeKind::Modified);
        assert_eq!(changes[2].id().as_str(), "fixtures/removed");
        assert_eq!(changes[2].change(), AssetChangeKind::Removed);

        let modified = AssetId::from_str("fixtures/modified")?;
        let removed = AssetId::from_str("fixtures/removed")?;
        let added = AssetId::from_str("fixtures/added")?;
        assert_eq!(old_snapshot.store().read_asset(&modified)?.as_ref(), b"old");
        assert_eq!(
            old_snapshot.store().read_asset(&removed)?.as_ref(),
            b"removed"
        );
        assert_eq!(
            reload.snapshot().store().read_asset(&modified)?.as_ref(),
            b"new"
        );
        assert_eq!(
            reload.snapshot().store().read_asset(&added)?.as_ref(),
            b"added"
        );
        assert!(reload.snapshot().store().read_asset(&removed).is_err());
        Ok(())
    }

    #[test]
    fn unchanged_hot_reload_keeps_the_generation() -> anyhow::Result<()> {
        let fixture = Fixture::new()?;
        fixture.asset("fixtures/example", "shared", "example.bin", b"bytes")?;
        let _cooked = fixture
            .pipeline
            .cook(&fixture.request("pak000", &["fixtures/example"])?)?;
        let manager = fixture.open_manager()?;
        let before = manager.snapshot();
        let reload = manager.reload()?;

        assert_eq!(reload.status(), AssetReloadStatus::Unchanged);
        assert_eq!(reload.snapshot().generation(), before.generation());
        assert_eq!(reload.snapshot().asset_set_hash(), before.asset_set_hash());
        assert!(reload.changes().is_empty());
        Ok(())
    }

    #[test]
    fn failed_hot_reload_preserves_the_current_snapshot() -> anyhow::Result<()> {
        let fixture = Fixture::new()?;
        fixture.asset("fixtures/example", "shared", "example.bin", b"bytes")?;
        let _initial = fixture
            .pipeline
            .cook(&fixture.request("pak000", &["fixtures/example"])?)?;
        let manager = fixture.open_manager()?;
        let before = manager.snapshot();

        fixture.asset("fixtures/example", "simulation", "example.bin", b"bytes")?;
        let _candidate = fixture
            .pipeline
            .cook(&fixture.request("pak000", &["fixtures/example"])?)?;
        let error = manager
            .reload()
            .err()
            .context("expected hot reload rejection")?;
        assert!(matches!(error, Error::HotReloadReclassification { .. }));

        let after = manager.snapshot();
        assert_eq!(after.generation(), before.generation());
        assert_eq!(after.asset_set_hash(), before.asset_set_hash());
        let asset = AssetId::from_str("fixtures/example")?;
        assert_eq!(
            after
                .store()
                .resolve(&asset)
                .context("missing preserved asset")?
                .record()
                .audience,
            blackflower_assets::AssetAudience::Shared
        );
        Ok(())
    }

    #[test]
    fn removed_assets_keep_their_contract_when_readded() -> anyhow::Result<()> {
        let fixture = Fixture::new()?;
        fixture.asset("fixtures/example", "shared", "example.bin", b"bytes")?;
        let _initial = fixture
            .pipeline
            .cook(&fixture.request("pak000", &["fixtures/example"])?)?;
        let manager = fixture.open_manager()?;

        let _removed = fixture.pipeline.cook(&fixture.request("pak000", &[])?)?;
        let removal = manager.reload()?;
        assert_eq!(removal.status(), AssetReloadStatus::Reloaded);
        assert_eq!(removal.snapshot().generation().get(), 1);

        fixture.asset("fixtures/example", "simulation", "example.bin", b"bytes")?;
        let _readded = fixture
            .pipeline
            .cook(&fixture.request("pak000", &["fixtures/example"])?)?;
        let error = manager
            .reload()
            .err()
            .context("expected historical contract rejection")?;
        assert!(matches!(error, Error::HotReloadReclassification { .. }));
        assert_eq!(manager.snapshot().generation().get(), 1);
        Ok(())
    }

    #[test]
    fn watcher_debounces_a_recook_and_publishes_the_new_snapshot() -> anyhow::Result<()> {
        let fixture = Fixture::new()?;
        fixture.asset("fixtures/example", "presentation", "example.bin", b"old")?;
        let _initial = fixture
            .pipeline
            .cook(&fixture.request("pak000", &["fixtures/example"])?)?;
        let manager = Arc::new(fixture.open_manager()?);
        let watcher = AssetStoreWatcher::watch(Arc::clone(&manager), Duration::from_millis(50))?;

        fixture.asset("fixtures/example", "presentation", "example.bin", b"new")?;
        let _changed = fixture
            .pipeline
            .cook(&fixture.request("pak000", &["fixtures/example"])?)?;

        let deadline = Instant::now() + Duration::from_secs(10);
        loop {
            let remaining = deadline.saturating_duration_since(Instant::now());
            if remaining.is_zero() {
                anyhow::bail!("timed out waiting for asset watcher reload");
            }
            let event = match watcher.events().recv_timeout(remaining) {
                Ok(event) => event,
                Err(RecvTimeoutError::Timeout) => {
                    anyhow::bail!("timed out waiting for asset watcher reload");
                }
                Err(RecvTimeoutError::Disconnected) => {
                    anyhow::bail!("asset watcher event channel disconnected");
                }
            };
            match event {
                AssetWatchEvent::Reloaded(reload)
                    if reload.status() == AssetReloadStatus::Reloaded =>
                {
                    let asset = AssetId::from_str("fixtures/example")?;
                    assert_eq!(reload.snapshot().generation().get(), 1);
                    assert_eq!(
                        reload.snapshot().store().read_asset(&asset)?.as_ref(),
                        b"new"
                    );
                    break;
                }
                AssetWatchEvent::Reloaded(_) => {}
                AssetWatchEvent::ReloadFailed(error) => return Err(error.into()),
                AssetWatchEvent::WatcherFailed(error) => return Err(error.into()),
                _ => {}
            }
        }

        Ok(())
    }

    #[test]
    fn watcher_reports_a_rejected_candidate_without_replacing_the_snapshot() -> anyhow::Result<()> {
        let fixture = Fixture::new()?;
        fixture.asset("fixtures/example", "shared", "example.bin", b"old")?;
        let _initial = fixture
            .pipeline
            .cook(&fixture.request("pak000", &["fixtures/example"])?)?;
        let manager = Arc::new(fixture.open_manager()?);
        let watcher = AssetStoreWatcher::watch(Arc::clone(&manager), Duration::from_millis(50))?;

        fixture.asset("fixtures/example", "simulation", "example.bin", b"new")?;
        let _candidate = fixture
            .pipeline
            .cook(&fixture.request("pak000", &["fixtures/example"])?)?;

        let event = watcher
            .events()
            .recv_timeout(Duration::from_secs(10))
            .context("timed out waiting for rejected watcher reload")?;
        assert!(matches!(
            event,
            AssetWatchEvent::ReloadFailed(Error::HotReloadReclassification { .. })
        ));
        let snapshot = manager.snapshot();
        let asset = AssetId::from_str("fixtures/example")?;
        assert_eq!(snapshot.generation().get(), 0);
        assert_eq!(
            snapshot
                .store()
                .resolve(&asset)
                .context("missing preserved asset")?
                .record()
                .audience,
            blackflower_assets::AssetAudience::Shared
        );
        assert_eq!(snapshot.store().read_asset(&asset)?.as_ref(), b"old");
        Ok(())
    }

    #[test]
    fn ignores_other_files_and_nested_packages() -> anyhow::Result<()> {
        let fixture = Fixture::new()?;
        fixture.asset("fixtures/example", "shared", "example.bin", b"bytes")?;
        let cooked = fixture
            .pipeline
            .cook(&fixture.request("pak000", &["fixtures/example"])?)?;
        fs::write(fixture.package_dir().join("notes.txt"), b"ignored")?;
        let nested = fixture.package_dir().join("nested");
        fs::create_dir_all(&nested)?;
        fs::copy(cooked.path, nested.join("pak999.squashfs"))?;
        let store = fixture.open_store()?;
        assert_eq!(store.packages().len(), 1);
        Ok(())
    }

    #[test]
    fn uses_reverse_lexical_not_natural_numeric_order() -> anyhow::Result<()> {
        let fixture = Fixture::new()?;
        fixture.asset("fixtures/example", "shared", "example.bin", b"base")?;
        let _base = fixture
            .pipeline
            .cook(&fixture.request("pak000", &["fixtures/example"])?)?;
        fixture.asset("fixtures/example", "shared", "example.bin", b"ten")?;
        let _ten = fixture
            .pipeline
            .cook(&fixture.request("pak10", &["fixtures/example"])?)?;
        fixture.asset("fixtures/example", "shared", "example.bin", b"two")?;
        let _two = fixture
            .pipeline
            .cook(&fixture.request("pak2", &["fixtures/example"])?)?;
        let store = fixture.open_store()?;
        let id = AssetId::from_str("fixtures/example")?;
        assert_eq!(store.read_asset(&id)?.as_ref(), b"two");
        assert_eq!(
            store
                .resolve(&id)
                .map(|resolved| resolved.package().name().as_str()),
            Some("pak2")
        );
        Ok(())
    }

    #[test]
    fn whitespace_only_manifest_changes_do_not_change_package_bytes() -> anyhow::Result<()> {
        let fixture = Fixture::new()?;
        fixture.asset("fixtures/example", "shared", "example.bin", b"bytes")?;
        let request = fixture.request("pak000", &["fixtures/example"])?;
        let first = fixture.pipeline.cook(&request)?;
        let manifest_path = fixture.source.join("fixtures/example/asset.toml");
        let original = fs::read_to_string(&manifest_path)?;
        fs::write(&manifest_path, format!("\n{original}\n"))?;
        let second = fixture.pipeline.cook(&request)?;
        assert_eq!(first.package_hash, second.package_hash);
        Ok(())
    }

    #[test]
    fn whitespace_only_profile_changes_do_not_change_package_bytes() -> anyhow::Result<()> {
        let fixture = Fixture::new()?;
        fixture.luau(
            "scripts/example",
            "simulation",
            "example.luau",
            b"return true\n",
        )?;
        let request = fixture.request("pak000", &["scripts/example"])?;
        let first = fixture.pipeline.cook(&request)?;
        let first_bytes = fs::read(&first.path)?;
        let path = fixture.profiles.join("debug.toml");
        let original = fs::read_to_string(&path)?;
        fs::write(&path, format!("\n{original}\n"))?;
        let second = fixture.pipeline.cook(&request)?;
        assert_eq!(first.package_hash, second.package_hash);
        assert_eq!(first_bytes, fs::read(second.path)?);
        Ok(())
    }

    #[test]
    fn corrupt_cache_objects_are_rebuilt() -> anyhow::Result<()> {
        let fixture = Fixture::new()?;
        fixture.asset("fixtures/example", "shared", "example.bin", b"correct")?;
        let request = fixture.request("pak000", &["fixtures/example"])?;
        let result = fixture.pipeline.cook(&request)?;
        let store = fixture.open_store()?;
        let id = AssetId::from_str("fixtures/example")?;
        let content_hash = store
            .resolve(&id)
            .context("missing cooked asset")?
            .record()
            .content_hash;
        let cache = fixture
            .target
            .join("asset-cache/objects/blake3")
            .join(content_hash.to_string());
        fs::write(&cache, b"corrupt")?;
        let _recooked = fixture.pipeline.cook(&request)?;
        assert_eq!(fs::read(cache)?, b"correct");
        assert_eq!(result.assets, 1);
        Ok(())
    }

    #[test]
    fn invalid_recook_preserves_the_previous_package() -> anyhow::Result<()> {
        let fixture = Fixture::new()?;
        fixture.asset("fixtures/example", "shared", "example.bin", b"valid")?;
        let request = fixture.request("pak000", &["fixtures/example"])?;
        let first = fixture.pipeline.cook(&request)?;
        let first_bytes = fs::read(&first.path)?;
        fs::write(
            fixture.source.join("fixtures/example/asset.toml"),
            "schema = 99\n",
        )?;
        assert!(fixture.pipeline.cook(&request).is_err());
        assert_eq!(fs::read(first.path)?, first_bytes);
        Ok(())
    }

    #[test]
    fn rejects_dependencies_in_source_asset_manifests() -> anyhow::Result<()> {
        let fixture = Fixture::new()?;
        fixture.write_manifest(
            "fixtures/example",
            "schema = 1\nid = \"fixtures/example\"\nkind = \"blob\"\naudience = \"shared\"\ndependencies = []\n\n[blob]\nsource = \"example.bin\"\n",
        )?;
        fs::write(
            fixture.source.join("fixtures/example/example.bin"),
            b"bytes",
        )?;
        assert!(fixture.pipeline.check().is_err());
        Ok(())
    }

    #[test]
    fn package_manifest_is_required_and_selects_exact_assets() -> anyhow::Result<()> {
        let fixture = Fixture::new()?;
        fixture.asset("fixtures/included", "shared", "included.bin", b"included")?;
        fixture.asset("fixtures/unlisted", "shared", "unlisted.bin", b"unlisted")?;

        let request = CookRequest {
            profile: ProfileName::from_str("debug")?,
            package: PackageName::from_str("pak000")?,
            signing_key: Fixture::signing_key(),
        };
        assert!(fixture.pipeline.cook(&request).is_err());

        fixture.package("pak000", &["fixtures/included"])?;
        let result = fixture.pipeline.cook(&request)?;
        assert_eq!(result.assets, 1);
        let store = fixture.open_store()?;
        assert!(
            store
                .resolve(&AssetId::from_str("fixtures/included")?)
                .is_some()
        );
        assert!(
            store
                .resolve(&AssetId::from_str("fixtures/unlisted")?)
                .is_none()
        );
        Ok(())
    }

    #[test]
    fn rejects_noncanonical_and_invalid_package_manifests() -> anyhow::Result<()> {
        let misplaced = Fixture::new()?;
        fs::create_dir_all(misplaced.source.join("misplaced"))?;
        fs::write(
            misplaced.source.join("misplaced/package.toml"),
            "schema = 1\nassets = []\n",
        )?;
        assert!(misplaced.pipeline.check().is_err());

        let unknown = Fixture::new()?;
        fs::create_dir_all(unknown.source.join("packages/pak000"))?;
        fs::write(
            unknown.source.join("packages/pak000/package.toml"),
            "schema = 1\nassets = []\nunknown = true\n",
        )?;
        assert!(unknown.pipeline.check().is_err());

        let missing_asset = Fixture::new()?;
        missing_asset.package("pak000", &["fixtures/missing"])?;
        assert!(missing_asset.pipeline.check().is_err());
        Ok(())
    }

    #[test]
    fn winning_dependencies_resolve_across_packages() -> anyhow::Result<()> {
        let fixture = Fixture::new()?;
        fixture.asset("fixtures/winner", "shared", "winner.bin", b"winner")?;
        fixture.asset("fixtures/lower", "shared", "lower.bin", b"lower")?;

        let base = fixture.write_selected_package("pak000", &["fixtures/lower"])?;
        let mut winner_catalog = fixture.selected_catalog(&["fixtures/winner"])?;
        let winner_record = winner_catalog
            .assets
            .first_mut()
            .context("expected winner record")?;
        winner_record
            .dependencies
            .push(AssetId::from_str("fixtures/lower")?);
        let _override = fixture.write_catalog_package("pak900", &winner_catalog)?;
        let store = fixture.open_store()?;
        let winner = AssetId::from_str("fixtures/winner")?;
        assert_eq!(store.read_asset(&winner)?.as_ref(), b"winner");

        fs::remove_file(base)?;
        let error = fixture
            .open_store()
            .err()
            .context("expected missing global dependency")?;
        assert!(matches!(error, Error::MissingDependency { .. }));
        Ok(())
    }

    #[test]
    fn animation_dependencies_must_resolve_to_a_skeleton() -> anyhow::Result<()> {
        let fixture = Fixture::new()?;
        fixture.asset("fixtures/clip", "presentation", "clip.bin", b"clip")?;
        fixture.asset("fixtures/not-a-rig", "presentation", "rig.bin", b"rig")?;
        let mut catalog = fixture.selected_catalog(&["fixtures/clip", "fixtures/not-a-rig"])?;
        let dependency = AssetId::from_str("fixtures/not-a-rig")?;
        let clip = catalog
            .assets
            .iter_mut()
            .find(|record| record.id.as_str() == "fixtures/clip")
            .context("missing clip fixture")?;
        clip.kind = AssetKind::AnimationClip;
        clip.dependencies.push(dependency);
        let _path = fixture.write_catalog_package("pak000", &catalog)?;

        let error = fixture
            .open_store()
            .err()
            .context("expected animation dependency kind rejection")?;
        assert!(matches!(error, Error::DependencyKindMismatch { .. }));
        Ok(())
    }

    #[test]
    fn rejects_traversal_and_unknown_manifest_fields() -> anyhow::Result<()> {
        let traversal = Fixture::new()?;
        traversal.write_manifest(
            "fixtures/example",
            "schema = 1\nid = \"fixtures/example\"\nkind = \"blob\"\naudience = \"shared\"\n\n[blob]\nsource = \"../outside.bin\"\n",
        )?;
        assert!(traversal.pipeline.check().is_err());

        let unknown = Fixture::new()?;
        unknown.write_manifest(
            "fixtures/example",
            "schema = 1\nid = \"fixtures/example\"\nkind = \"blob\"\naudience = \"shared\"\nunknown = true\n\n[blob]\nsource = \"example.bin\"\n",
        )?;
        fs::write(
            unknown.source.join("fixtures/example/example.bin"),
            b"bytes",
        )?;
        assert!(unknown.pipeline.check().is_err());
        Ok(())
    }

    #[test]
    fn rejects_shader_profile_overrides_and_non_presentation_manifests() -> anyhow::Result<()> {
        let override_setting = Fixture::new()?;
        override_setting.shader(
            "shaders/override",
            "presentation",
            "override.slang",
            b"float4 main(float4 position : POSITION) : SV_Position { return position; }\n",
            "main",
            "vertex",
        )?;
        override_setting.write_manifest(
            "shaders/override",
            "schema = 1\nid = \"shaders/override\"\nkind = \"shader_module\"\naudience = \"presentation\"\n\n[shader]\nsource = \"override.slang\"\nentry_point = \"main\"\nstage = \"vertex\"\ntarget = \"spirv\"\n",
        )?;
        assert!(override_setting.pipeline.check().is_err());

        let simulation = Fixture::new()?;
        simulation.shader(
            "shaders/simulation",
            "simulation",
            "simulation.slang",
            b"float4 main(float4 position : POSITION) : SV_Position { return position; }\n",
            "main",
            "vertex",
        )?;
        let error = simulation
            .pipeline
            .check()
            .err()
            .context("expected shader audience rejection")?;
        assert!(format!("{error:#}").contains("must use audience `presentation`"));

        let invalid_entry_point = Fixture::new()?;
        invalid_entry_point.shader(
            "shaders/entry",
            "presentation",
            "entry.slang",
            b"float4 main(float4 position : POSITION) : SV_Position { return position; }\n",
            "not portable",
            "vertex",
        )?;
        let error = invalid_entry_point
            .pipeline
            .check()
            .err()
            .context("expected shader entry-point rejection")?;
        assert!(format!("{error:#}").contains("must be a portable identifier"));
        Ok(())
    }

    #[cfg(unix)]
    #[test]
    fn rejects_source_symlinks_that_escape_the_source_root() -> anyhow::Result<()> {
        use std::os::unix::fs::symlink;

        let fixture = Fixture::new()?;
        fixture.write_manifest(
            "fixtures/example",
            "schema = 1\nid = \"fixtures/example\"\nkind = \"blob\"\naudience = \"shared\"\n\n[blob]\nsource = \"example.bin\"\n",
        )?;
        let outside = fixture._temp.path().join("outside.bin");
        fs::write(&outside, b"outside")?;
        symlink(outside, fixture.source.join("fixtures/example/example.bin"))?;
        assert!(fixture.pipeline.check().is_err());
        Ok(())
    }

    #[test]
    fn asset_content_is_verified_when_read() -> anyhow::Result<()> {
        let fixture = Fixture::new()?;
        fixture.asset("fixtures/example", "shared", "example.bin", b"bytes")?;
        let mut catalog = fixture.selected_catalog(&["fixtures/example"])?;
        let record = catalog
            .assets
            .first_mut()
            .context("expected selected record")?;
        record.content_hash = ContentHash::hash_bytes(b"other");
        record.object_path = format!("objects/blake3/{}", record.content_hash);
        let _package = fixture.write_catalog_package("pak000", &catalog)?;

        let store = fixture.open_store()?;
        let asset = AssetId::from_str("fixtures/example")?;
        assert!(store.read_asset(&asset).is_err());
        Ok(())
    }

    #[test]
    fn rejects_corrupt_and_invalidly_named_packages() -> anyhow::Result<()> {
        let fixture = Fixture::new()?;
        fs::create_dir_all(fixture.package_dir())?;
        fs::write(
            fixture.package_dir().join("pak999.squashfs"),
            b"not squashfs",
        )?;
        assert!(fixture.open_store().is_err());
        fs::remove_file(fixture.package_dir().join("pak999.squashfs"))?;
        fs::write(
            fixture.package_dir().join("Pak999.squashfs"),
            b"not squashfs",
        )?;
        let error = fixture
            .open_store()
            .err()
            .context("expected invalid package name")?;
        assert!(matches!(error, Error::InvalidPackageName(_)));
        Ok(())
    }

    #[test]
    fn rejects_unsigned_and_untrusted_packages() -> anyhow::Result<()> {
        let fixture = Fixture::new()?;
        fixture.asset("fixtures/example", "shared", "example.bin", b"bytes")?;
        let catalog = fixture.selected_catalog(&["fixtures/example"])?;
        let cooked = fixture.cooked_for_catalog(&catalog)?;
        let directory = fixture.package_dir();
        fs::create_dir_all(&directory)?;
        let path = directory.join("pak000.squashfs");
        write_package(&path, &catalog, &cooked)?;

        let unsigned = fixture
            .open_store()
            .err()
            .context("expected unsigned package rejection")?;
        assert!(matches!(unsigned, Error::InvalidSignatureFooter { .. }));

        let other_key = AssetSigningKey::from_bytes(&[0x24; 32]);
        let _payload_hash = sign_package(&path, &other_key)?;
        let untrusted = fixture
            .open_store()
            .err()
            .context("expected untrusted signer rejection")?;
        assert!(matches!(untrusted, Error::UntrustedSigningKey { .. }));
        Ok(())
    }

    #[test]
    fn rejects_tampered_payload_and_signature() -> anyhow::Result<()> {
        let fixture = Fixture::new()?;
        fixture.asset("fixtures/example", "shared", "example.bin", b"bytes")?;
        let cooked = fixture
            .pipeline
            .cook(&fixture.request("pak000", &["fixtures/example"])?)?;
        let mut bytes = fs::read(&cooked.path)?;
        let payload_byte = bytes.get_mut(128).context("package payload is too short")?;
        *payload_byte ^= 0x01;
        fs::write(&cooked.path, &bytes)?;
        let payload_error = fixture
            .open_store()
            .err()
            .context("expected payload tamper rejection")?;
        assert!(matches!(
            payload_error,
            Error::InvalidSignatureFooter { .. }
        ));

        let recooked = fixture
            .pipeline
            .cook(&fixture.request("pak000", &["fixtures/example"])?)?;
        let mut bytes = fs::read(&recooked.path)?;
        let signature_byte = bytes.last_mut().context("signed package is empty")?;
        *signature_byte ^= 0x01;
        fs::write(&recooked.path, bytes)?;
        let signature_error = fixture
            .open_store()
            .err()
            .context("expected signature tamper rejection")?;
        assert!(matches!(
            signature_error,
            Error::InvalidPackageSignature { .. }
        ));
        Ok(())
    }

    #[test]
    fn payload_hash_is_stable_across_signing_keys() -> anyhow::Result<()> {
        let fixture = Fixture::new()?;
        fixture.asset("fixtures/example", "shared", "example.bin", b"bytes")?;
        let first = fixture
            .pipeline
            .cook(&fixture.request("pak000", &["fixtures/example"])?)?;
        fixture.package("pak100", &["fixtures/example"])?;
        let second_request = CookRequest {
            profile: ProfileName::from_str("debug")?,
            package: PackageName::from_str("pak100")?,
            signing_key: AssetSigningKey::from_bytes(&[0x24; 32]),
        };
        let second = fixture.pipeline.cook(&second_request)?;
        let trust_store = AssetTrustStore::from_public_keys([
            Fixture::signing_key().public_key_bytes(),
            second_request.signing_key.public_key_bytes(),
        ])?;
        let first = AssetPackage::open(first.path, &trust_store)?;
        let second = AssetPackage::open(second.path, &trust_store)?;
        assert_eq!(first.payload_hash(), second.payload_hash());
        assert_ne!(first.hash(), second.hash());
        assert_ne!(first.signing_key_id(), second.signing_key_id());
        Ok(())
    }

    struct Fixture {
        _temp: TempDir,
        profiles: std::path::PathBuf,
        source: std::path::PathBuf,
        target: std::path::PathBuf,
        pipeline: Pipeline,
    }

    impl Fixture {
        fn new() -> anyhow::Result<Self> {
            let temp = TempDir::new()?;
            let profiles = temp.path().join("assets/profiles");
            let source = temp.path().join("assets/source");
            let target = temp.path().join("target");
            fs::create_dir_all(&profiles)?;
            fs::create_dir_all(&source)?;
            fs::write(profiles.join("debug.toml"), DEBUG_PROFILE)?;
            let pipeline = Pipeline::new(profiles.clone(), source.clone(), target.clone());
            Ok(Self {
                _temp: temp,
                profiles,
                source,
                target,
                pipeline,
            })
        }

        fn asset(
            &self,
            id: &str,
            audience: &str,
            source_name: &str,
            bytes: &[u8],
        ) -> anyhow::Result<()> {
            let directory = self.source.join(id);
            fs::create_dir_all(&directory)?;
            fs::write(directory.join(source_name), bytes)?;
            let manifest = format!(
                "schema = 1\nid = \"{id}\"\nkind = \"blob\"\naudience = \"{audience}\"\n\n[blob]\nsource = \"{source_name}\"\n"
            );
            fs::write(directory.join("asset.toml"), manifest)?;
            Ok(())
        }

        fn luau(
            &self,
            id: &str,
            audience: &str,
            source_name: &str,
            bytes: &[u8],
        ) -> anyhow::Result<()> {
            let directory = self.source.join(id);
            fs::create_dir_all(&directory)?;
            fs::write(directory.join(source_name), bytes)?;
            let manifest = format!(
                "schema = 1\nid = \"{id}\"\nkind = \"luau_bytecode\"\naudience = \"{audience}\"\n\n[luau]\nsource = \"{source_name}\"\n"
            );
            fs::write(directory.join("asset.toml"), manifest)?;
            Ok(())
        }

        fn shader(
            &self,
            id: &str,
            audience: &str,
            source_name: &str,
            bytes: &[u8],
            entry_point: &str,
            stage: &str,
        ) -> anyhow::Result<()> {
            let directory = self.source.join(id);
            fs::create_dir_all(&directory)?;
            fs::write(directory.join(source_name), bytes)?;
            let manifest = format!(
                "schema = 1\nid = \"{id}\"\nkind = \"shader_module\"\naudience = \"{audience}\"\n\n[shader]\nsource = \"{source_name}\"\nentry_point = \"{entry_point}\"\nstage = \"{stage}\"\n"
            );
            fs::write(directory.join("asset.toml"), manifest)?;
            Ok(())
        }

        fn texture(
            &self,
            id: &str,
            audience: &str,
            source_name: &str,
            bytes: &[u8],
            semantic: &str,
        ) -> anyhow::Result<()> {
            let directory = self.source.join(id);
            fs::create_dir_all(&directory)?;
            fs::write(directory.join(source_name), bytes)?;
            let manifest = format!(
                "schema = 1\nid = \"{id}\"\nkind = \"texture2d\"\naudience = \"{audience}\"\n\n[texture]\nsource = \"{source_name}\"\nsemantic = \"{semantic}\"\n"
            );
            fs::write(directory.join("asset.toml"), manifest)?;
            Ok(())
        }

        #[allow(
            clippy::too_many_arguments,
            reason = "test fixture mirrors the strict mesh manifest"
        )]
        fn mesh(
            &self,
            id: &str,
            audience: &str,
            source_name: &str,
            source_bytes: &[u8],
            buffer_name: &str,
            buffer_bytes: &[u8],
            mesh_name: &str,
        ) -> anyhow::Result<()> {
            let directory = self.source.join(id);
            fs::create_dir_all(&directory)?;
            fs::write(directory.join(source_name), source_bytes)?;
            fs::write(directory.join(buffer_name), buffer_bytes)?;
            let manifest = format!(
                "schema = 1\nid = \"{id}\"\nkind = \"mesh\"\naudience = \"{audience}\"\n\n[mesh]\nsource = \"{source_name}\"\nmesh = \"{mesh_name}\"\n"
            );
            fs::write(directory.join("asset.toml"), manifest)?;
            Ok(())
        }

        fn write_profile(&self, name: &str, profile: &str) -> anyhow::Result<()> {
            fs::write(self.profiles.join(format!("{name}.toml")), profile)?;
            Ok(())
        }

        fn write_manifest(&self, id: &str, manifest: &str) -> anyhow::Result<()> {
            let directory = self.source.join(id);
            fs::create_dir_all(&directory)?;
            fs::write(directory.join("asset.toml"), manifest)?;
            Ok(())
        }

        fn package(&self, name: &str, assets: &[&str]) -> anyhow::Result<()> {
            let directory = self.source.join("packages").join(name);
            fs::create_dir_all(&directory)?;
            let assets = assets
                .iter()
                .map(|asset| format!("\"{asset}\""))
                .collect::<Vec<_>>()
                .join(", ");
            let manifest = format!("schema = 1\nassets = [{assets}]\n");
            fs::write(directory.join("package.toml"), manifest)?;
            Ok(())
        }

        fn request(&self, package: &str, assets: &[&str]) -> anyhow::Result<CookRequest> {
            self.package(package, assets)?;
            Ok(CookRequest {
                profile: ProfileName::from_str("debug")?,
                package: PackageName::from_str(package)?,
                signing_key: Self::signing_key(),
            })
        }

        fn package_dir(&self) -> std::path::PathBuf {
            self.target.join("assets/packages/debug")
        }

        fn signing_key() -> AssetSigningKey {
            AssetSigningKey::from_bytes(&TEST_SIGNING_SECRET)
        }

        fn trust_store(&self) -> Result<AssetTrustStore, Error> {
            AssetTrustStore::from_public_keys([Self::signing_key().public_key_bytes()])
        }

        fn open_store(&self) -> Result<AssetStore, Error> {
            let trust_store = self.trust_store()?;
            AssetStore::open_dir(self.package_dir(), &trust_store)
        }

        fn open_manager(&self) -> Result<AssetStoreManager, Error> {
            AssetStoreManager::open_dir(self.package_dir(), self.trust_store()?)
        }

        fn selected_catalog(&self, selected: &[&str]) -> anyhow::Result<AssetCatalog> {
            let repository = Repository::load(&self.source)?;
            let selected = selected
                .iter()
                .map(|id| AssetId::from_str(id))
                .collect::<Result<BTreeSet<_>, _>>()?;
            let profiles = CookingProfiles::load(&self.profiles)?;
            let profile_name = ProfileName::from_str("debug")?;
            let profile = profiles.get(&profile_name)?;
            let cooked = cook_assets(&repository, &selected, profile)?;
            let toolchain = toolchain_identity();
            build_catalog(&cooked, profile.identity.clone(), toolchain)
        }

        fn write_selected_package(
            &self,
            package: &str,
            selected: &[&str],
        ) -> anyhow::Result<std::path::PathBuf> {
            let catalog = self.selected_catalog(selected)?;
            self.write_catalog_package(package, &catalog)
        }

        fn write_catalog_package(
            &self,
            package: &str,
            catalog: &AssetCatalog,
        ) -> anyhow::Result<std::path::PathBuf> {
            let cooked = self.cooked_for_catalog(catalog)?;
            let directory = self.package_dir();
            fs::create_dir_all(&directory)?;
            let name = PackageName::from_str(package)?;
            let path = directory.join(name.file_name());
            write_package(&path, catalog, &cooked)?;
            let _payload_hash = sign_package(&path, &Self::signing_key())?;
            Ok(path)
        }

        fn cooked_for_catalog(
            &self,
            catalog: &AssetCatalog,
        ) -> anyhow::Result<BTreeMap<AssetId, CookedAsset>> {
            let repository = Repository::load(&self.source)?;
            let selected = catalog
                .assets
                .iter()
                .map(|record| record.id.clone())
                .collect::<BTreeSet<_>>();
            let profiles = CookingProfiles::load(&self.profiles)?;
            let profile = profiles.get(&catalog.profile.name)?;
            cook_assets(&repository, &selected, profile)
        }
    }

    fn png_texture() -> anyhow::Result<Vec<u8>> {
        let image = image::RgbaImage::from_fn(3, 2, |x, y| {
            if (x + y) % 2 == 0 {
                image::Rgba([255, 64, 16, 255])
            } else {
                image::Rgba([8, 96, 224, 192])
            }
        });
        let mut output = std::io::Cursor::new(Vec::new());
        image::DynamicImage::ImageRgba8(image).write_to(&mut output, image::ImageFormat::Png)?;
        Ok(output.into_inner())
    }

    fn exr_texture() -> anyhow::Result<Vec<u8>> {
        let image = image::Rgba32FImage::from_fn(2, 2, |x, y| {
            let scale = if (x + y) % 2 == 0 { 4.0 } else { 0.25 };
            image::Rgba([scale, scale * 0.5, scale * 0.125, 1.0])
        });
        let mut output = std::io::Cursor::new(Vec::new());
        image::DynamicImage::ImageRgba32F(image)
            .write_to(&mut output, image::ImageFormat::OpenExr)?;
        Ok(output.into_inner())
    }

    fn rigged_animation_gltf() -> anyhow::Result<Vec<u8>> {
        let path = std::path::Path::new(env!("CARGO_MANIFEST_DIR")).join(
            "../../crates/blackflower-animation/vendor/ozz-animation/media/gltf/khronos/rigged_simple.gltf",
        );
        let mut document: serde_json::Value = serde_json::from_slice(&fs::read(path)?)?;
        document["animations"][0]["name"] = serde_json::json!("Walk");
        document["animations"][0]["extras"]["blackflower"] = serde_json::json!({
            "schema": 1,
            "loop": true,
            "additive": {"enabled": false, "reference": "animation"},
            "root_motion": {
                "enabled": true,
                "joint": "Bone",
                "translation_axes": ["x", "z"],
                "rotation_axes": ["y"],
                "reference": "skeleton",
                "remove_from_pose": true,
                "loop_correction": true
            },
            "markers": [
                {"name": "end", "time_seconds": 2.083333015441895},
                {"name": "start", "time_seconds": 0.0},
                {"name": "middle", "time_seconds": 0.5}
            ]
        });
        let mut additive = document["animations"][0].clone();
        additive["name"] = serde_json::json!("Lean");
        additive["extras"]["blackflower"] = serde_json::json!({
            "schema": 1,
            "loop": false,
            "additive": {"enabled": true, "reference": "skeleton"},
            "root_motion": {"enabled": false},
            "markers": []
        });
        document["animations"]
            .as_array_mut()
            .context("fixture animations are not an array")?
            .push(additive);
        Ok(serde_json::to_vec(&document)?)
    }

    fn grid_gltf(side: u16) -> anyhow::Result<(Vec<u8>, Vec<u8>)> {
        let (buffer, position_bytes, index_bytes) = grid_buffer(side);
        let document = grid_document(side, buffer.len(), position_bytes, index_bytes);
        Ok((serde_json::to_vec(&document)?, buffer))
    }

    #[allow(
        clippy::too_many_lines,
        reason = "the self-contained glTF fixture spells out the complete binary accessor layout"
    )]
    fn navigation_floor_gltf() -> anyhow::Result<(Vec<u8>, Vec<u8>)> {
        let mut buffer = Vec::new();
        for value in [
            0.0_f32, 0.0, 0.0, 0.0, 0.0, 10.0, 10.0, 0.0, 10.0, 10.0, 0.0, 0.0,
        ] {
            buffer.extend_from_slice(&value.to_le_bytes());
        }
        for index in [0_u32, 1, 2, 0, 2, 3] {
            buffer.extend_from_slice(&index.to_le_bytes());
        }
        let document = serde_json::json!({
            "asset": {"version": "2.0"},
            "scene": 0,
            "scenes": [{"nodes": [0]}],
            "nodes": [{
                "name": "Navigation Floor",
                "mesh": 0,
                "extras": {"blackflower": {
                    "schema": 1,
                    "node": {
                        "kind": "navigation_surface",
                        "id": "floor_main"
                    },
                    "navigation": {
                        "role": "surface",
                        "area_key": "ground"
                    }
                }}
            }],
            "meshes": [{"primitives": [{
                "attributes": {"POSITION": 0},
                "indices": 1,
                "mode": 4
            }]}],
            "buffers": [{"uri": "navigation.bin", "byteLength": buffer.len()}],
            "bufferViews": [
                {"buffer": 0, "byteOffset": 0, "byteLength": 48},
                {"buffer": 0, "byteOffset": 48, "byteLength": 24}
            ],
            "accessors": [
                {
                    "bufferView": 0,
                    "componentType": 5126,
                    "count": 4,
                    "type": "VEC3",
                    "min": [0, 0, 0],
                    "max": [10, 0, 10]
                },
                {
                    "bufferView": 1,
                    "componentType": 5125,
                    "count": 6,
                    "type": "SCALAR"
                }
            ]
        });
        Ok((serde_json::to_vec(&document)?, buffer))
    }

    fn grid_buffer(side: u16) -> (Vec<u8>, usize, usize) {
        let mut buffer = Vec::new();
        for y in 0..side {
            for x in 0..side {
                for value in [f32::from(x), f32::from(y), 0.0] {
                    buffer.extend_from_slice(&value.to_le_bytes());
                }
            }
        }
        let position_bytes = buffer.len();
        for y in 0..(side - 1) {
            for x in 0..(side - 1) {
                let first = u32::from(y) * u32::from(side) + u32::from(x);
                let next_row = first + u32::from(side);
                for index in [
                    first,
                    first + 1,
                    next_row + 1,
                    first,
                    next_row + 1,
                    next_row,
                ] {
                    buffer.extend_from_slice(&index.to_le_bytes());
                }
            }
        }
        let index_bytes = buffer.len() - position_bytes;
        (buffer, position_bytes, index_bytes)
    }

    fn grid_document(
        side: u16,
        buffer_bytes: usize,
        position_bytes: usize,
        index_bytes: usize,
    ) -> serde_json::Value {
        let vertex_count = u32::from(side) * u32::from(side);
        let cell_count = u32::from(side - 1) * u32::from(side - 1);
        let index_count = cell_count * 6;
        let maximum = f32::from(side - 1);
        serde_json::json!({
            "asset": { "version": "2.0" },
            "buffers": [{ "uri": "grid.bin", "byteLength": buffer_bytes }],
            "bufferViews": [
                {
                    "buffer": 0,
                    "byteOffset": 0,
                    "byteLength": position_bytes,
                    "target": 34962
                },
                {
                    "buffer": 0,
                    "byteOffset": position_bytes,
                    "byteLength": index_bytes,
                    "target": 34963
                }
            ],
            "accessors": [
                {
                    "bufferView": 0,
                    "componentType": 5126,
                    "count": vertex_count,
                    "type": "VEC3",
                    "min": [0.0, 0.0, 0.0],
                    "max": [maximum, maximum, 0.0]
                },
                {
                    "bufferView": 1,
                    "componentType": 5125,
                    "count": index_count,
                    "type": "SCALAR"
                }
            ],
            "meshes": [{
                "name": "Grid",
                "primitives": [{
                    "attributes": { "POSITION": 0 },
                    "indices": 1,
                    "mode": 4
                }]
            }]
        })
    }
}
