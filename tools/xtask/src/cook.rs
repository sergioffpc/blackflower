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

use crate::asset_cooker::{CookedAsset, cook_assets};
use crate::manifest::{Repository, SOURCE_SCHEMA};
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
        let repository = Repository::load(&self.source_root)?;
        Ok(CheckResult {
            profiles: profiles.len(),
            assets: repository.assets.len(),
            packages: repository.packages.len(),
        })
    }

    pub(crate) fn cook(&self, request: &CookRequest) -> anyhow::Result<CookResult> {
        let profiles = CookingProfiles::load(&self.profiles_root)?;
        let profile = profiles.get(&request.profile)?;
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
    use blackflower_assets::{
        AssetCatalog, AssetChangeKind, AssetId, AssetPackage, AssetReloadStatus, AssetSigningKey,
        AssetStore, AssetStoreManager, AssetStoreWatcher, AssetTrustStore, AssetWatchEvent, Bytes,
        ContentHash, Error, PackageName, ProfileName, sign_package,
    };
    use blackflower_scripting::{Bytecode, Runtime, Value};
    use tempfile::TempDir;

    use crate::asset_cooker::{CookedAsset, cook_assets};
    use crate::manifest::Repository;
    use crate::profile::CookingProfiles;

    use super::{CookRequest, Pipeline, build_catalog, toolchain_identity, write_package};

    const TEST_SIGNING_SECRET: [u8; 32] = [0x42; 32];
    const DEBUG_PROFILE: &str = "schema = 1\n\n[scripting.luau]\noptimization = \"baseline\"\ndebug = \"full\"\ntype_info = \"native_modules\"\n";
    const RELEASE_PROFILE: &str = "schema = 1\n\n[scripting.luau]\noptimization = \"aggressive\"\ndebug = \"line_info\"\ntype_info = \"native_modules\"\n";

    #[test]
    fn cooks_deterministically_and_reuses_the_logical_name() -> anyhow::Result<()> {
        let fixture = Fixture::new()?;
        fixture.asset("fixtures/example", "shared", "example.bin", b"first")?;
        let request = fixture.request("pak000", &["fixtures/example"])?;
        let first = fixture.pipeline.cook(&request)?;
        assert_eq!(
            first.package_hash.to_string(),
            "b46b70a5249b345c8345fb574725816f5f9057c51bb539d2d60407fd857f0e03"
        );
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

        fixture.write_profile("debug", RELEASE_PROFILE)?;
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
    fn rejects_unknown_or_malformed_cooking_profile_settings() -> anyhow::Result<()> {
        let unknown = Fixture::new()?;
        unknown.write_profile(
            "debug",
            "schema = 1\nunknown = true\n\n[scripting.luau]\noptimization = \"aggressive\"\ndebug = \"none\"\ntype_info = \"native_modules\"\n",
        )?;
        assert!(unknown.pipeline.check().is_err());

        let invalid_value = Fixture::new()?;
        invalid_value.write_profile(
            "debug",
            "schema = 1\n\n[scripting.luau]\noptimization = \"fastest\"\ndebug = \"none\"\ntype_info = \"native_modules\"\n",
        )?;
        assert!(invalid_value.pipeline.check().is_err());

        let removed_coverage = Fixture::new()?;
        removed_coverage.write_profile(
            "debug",
            "schema = 1\n\n[scripting.luau]\noptimization = \"aggressive\"\ndebug = \"none\"\ntype_info = \"native_modules\"\ncoverage = \"none\"\n",
        )?;
        assert!(removed_coverage.pipeline.check().is_err());
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
}
