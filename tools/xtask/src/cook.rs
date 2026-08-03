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
use blackflower_scripting_luau::luau_version;
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
    pub(crate) maps: usize,
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

    pub(crate) fn check(&self) -> anyhow::Result<CheckResult> {
        let profiles = CookingProfiles::load(&self.profiles_root)?;
        let gltf = gltf_source::validate_tree(&self.source_root)?;
        let repository = Repository::load(&self.source_root)?;
        Ok(CheckResult {
            profiles: profiles.len(),
            assets: repository.assets.len(),
            maps: repository.maps.len(),
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
            let recipe_path = recipe_root.join(format!("{}.toml", record.recipe_hash));
            let recipe = CachedRecipe {
                schema: SOURCE_SCHEMA,
                id: &record.id,
                content_hash: record.content_hash,
                recipe_hash: record.recipe_hash,
            };
            let bytes = crate::canonical_toml::encode(&recipe)?;
            write_atomic(&recipe_path, &bytes)?;
        }
        Ok(())
    }
}

#[allow(
    clippy::too_many_lines,
    reason = "the catalog records one explicit identity entry for every cooker toolchain"
)]
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
        audio: format!(
            "hound/{}+claxon/{}+rubato/{};flac-stream=pass-through;bfaudio={};bfsound={};{}",
            blackflower_audio_media::HOUND_VERSION,
            blackflower_audio_media::CLAXON_VERSION,
            blackflower_audio_media::RUBATO_VERSION,
            blackflower_audio_media::AUDIO_CLIP_SCHEMA,
            blackflower_audio_media::SOUND_EVENT_SCHEMA,
            blackflower_audio_media::COOKER_RECIPE,
        ),
        steam_audio_acoustics: steam_audio_identity(),
        acoustics_cooker_platform: crate::acoustic_cooker::platform_identity(),
        authoritative_acoustics: format!(
            "bfacmat/bfactpl/bfacpfb/bfacsim/bfacprf={};{}",
            blackflower_acoustics::ACOUSTIC_ASSET_SCHEMA,
            blackflower_cooker_acoustics::AUTHORITATIVE_COOKER_RECIPE,
        ),
    }
}

fn steam_audio_identity() -> String {
    format!(
        "steam-audio/{}.{}.{}@{};bfac={};{}",
        blackflower_audio_spatial::STEAM_AUDIO_VERSION.0,
        blackflower_audio_spatial::STEAM_AUDIO_VERSION.1,
        blackflower_audio_spatial::STEAM_AUDIO_VERSION.2,
        blackflower_cooker_acoustics::STEAM_AUDIO_REVISION,
        blackflower_audio_spatial::ACOUSTIC_ASSET_SCHEMA,
        blackflower_cooker_acoustics::COOKER_RECIPE,
    )
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
    let catalog_bytes = crate::canonical_toml::encode(catalog)?;
    let objects = unique_objects(catalog, cooked)?;

    let mut writer = configured_writer()?;
    let directory = NodeHeader::new(DIR_MODE, 0, 0, 0);
    let file = NodeHeader::new(FILE_MODE, 0, 0, 0);
    writer.push_dir("blackflower", directory)?;
    writer.push_dir("objects", directory)?;
    writer.push_dir("objects/blake3", directory)?;
    writer.push_file(Cursor::new(catalog_bytes), "blackflower/catalog.toml", file)?;
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
#[path = "../tests/unit/cook.rs"]
mod tests;
