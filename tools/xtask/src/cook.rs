use std::collections::{BTreeMap, BTreeSet};
use std::fs::{self, File};
use std::io::{Cursor, Write};
use std::path::{Path, PathBuf};

use anyhow::{Context, bail};
use backhand::compression::{CompressionOptions, Compressor, Zstd};
use backhand::{FilesystemCompressor, FilesystemWriter, NodeHeader};
use blackflower_assets::{
    ASSET_CATALOG_SCHEMA, AssetCatalog, AssetId, AssetPackage, AssetRecord, AssetSigningKey,
    AssetTrustStore, ContentHash, PackageHash, PackageName, RecipeHash, ToolchainIdentity,
    sign_package,
};
use serde::Serialize;
use tempfile::{NamedTempFile, TempDir};

use crate::manifest::{Repository, SOURCE_SCHEMA};

const DEFAULT_PROFILE: &str = "desktop-universal";
const BLOCK_SIZE: u32 = 128 * 1024;
const ZSTD_LEVEL: u32 = 3;
const DIR_MODE: u16 = 0o555;
const FILE_MODE: u16 = 0o444;

#[derive(Debug)]
pub(crate) struct Pipeline {
    source_root: PathBuf,
    target_root: PathBuf,
}

#[derive(Debug)]
pub(crate) struct CookRequest {
    pub(crate) profile: String,
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
    pub(crate) assets: usize,
    pub(crate) packages: usize,
}

impl Pipeline {
    pub(crate) fn for_workspace(workspace_root: &Path) -> Self {
        Self {
            source_root: workspace_root.join("assets/source"),
            target_root: workspace_root.join("target"),
        }
    }

    #[cfg(test)]
    fn new(source_root: PathBuf, target_root: PathBuf) -> Self {
        Self {
            source_root,
            target_root,
        }
    }

    pub(crate) fn check(&self) -> anyhow::Result<CheckResult> {
        let repository = Repository::load(&self.source_root)?;
        Ok(CheckResult {
            assets: repository.assets.len(),
            packages: repository.packages.len(),
        })
    }

    pub(crate) fn cook(&self, request: &CookRequest) -> anyhow::Result<CookResult> {
        validate_profile(&request.profile)?;
        let repository = Repository::load(&self.source_root)?;
        let selected = repository.selected_assets(&request.package)?;
        let toolchain = toolchain_identity();
        let toolchain_bytes = serde_json::to_vec(&toolchain)?;
        let recipe_hashes = repository.recipe_hashes(&request.profile, &toolchain_bytes)?;
        let catalog = build_catalog(
            &repository,
            &selected,
            &recipe_hashes,
            &request.profile,
            toolchain,
        )?;
        self.populate_cache(&repository, &catalog)?;
        let package_dir = self
            .target_root
            .join("assets/packages")
            .join(&request.profile);
        fs::create_dir_all(&package_dir)
            .with_context(|| format!("failed to create `{}`", package_dir.display()))?;
        let output_path = package_dir.join(request.package.file_name());
        let package_hash = write_and_publish_package(
            &package_dir,
            &output_path,
            &catalog,
            &repository,
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
        repository: &Repository,
        catalog: &AssetCatalog,
    ) -> anyhow::Result<()> {
        let object_root = self.target_root.join("asset-cache/objects/blake3");
        let recipe_root = self.target_root.join("asset-cache/recipes");
        fs::create_dir_all(&object_root)?;
        fs::create_dir_all(&recipe_root)?;
        for record in &catalog.assets {
            let asset = repository
                .assets
                .get(&record.id)
                .with_context(|| format!("missing loaded asset `{}`", record.id))?;
            let object_path = object_root.join(record.content_hash.to_string());
            write_cache_object(&object_path, &asset.source_bytes, record.content_hash)?;
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

fn validate_profile(profile: &str) -> anyhow::Result<()> {
    if profile != DEFAULT_PROFILE {
        bail!("unsupported asset profile `{profile}`");
    }
    Ok(())
}

fn toolchain_identity() -> ToolchainIdentity {
    ToolchainIdentity {
        cooker: format!("xtask/{}", env!("CARGO_PKG_VERSION")),
        squashfs: "backhand/0.25.1".to_owned(),
        archive:
            "squashfs-4.0-le;block=131072;zstd=3;epoch=0;uid=0;gid=0;signature=ed25519-blake3-v1"
                .to_owned(),
    }
}

fn build_catalog(
    repository: &Repository,
    selected: &BTreeSet<AssetId>,
    recipe_hashes: &BTreeMap<AssetId, RecipeHash>,
    profile: &str,
    toolchain: ToolchainIdentity,
) -> anyhow::Result<AssetCatalog> {
    let mut assets = Vec::with_capacity(selected.len());
    for id in selected {
        let asset = repository
            .assets
            .get(id)
            .with_context(|| format!("missing selected asset `{id}`"))?;
        let recipe_hash = recipe_hashes
            .get(id)
            .copied()
            .with_context(|| format!("missing recipe hash for `{id}`"))?;
        let byte_len =
            u64::try_from(asset.source_bytes.len()).context("asset length does not fit u64")?;
        assets.push(AssetRecord {
            id: id.clone(),
            kind: asset.manifest.kind,
            audience: asset.manifest.audience,
            dependencies: Vec::new(),
            content_hash: asset.content_hash,
            recipe_hash,
            byte_len,
            object_path: format!("objects/blake3/{}", asset.content_hash),
        });
    }
    Ok(AssetCatalog {
        schema: ASSET_CATALOG_SCHEMA,
        profile: profile.to_owned(),
        toolchain,
        assets,
    })
}

fn write_and_publish_package(
    package_dir: &Path,
    output_path: &Path,
    catalog: &AssetCatalog,
    repository: &Repository,
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
    write_package(&candidate, catalog, repository)?;
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
    repository: &Repository,
) -> anyhow::Result<()> {
    let mut catalog_bytes = serde_json::to_vec(catalog)?;
    catalog_bytes.push(b'\n');
    let objects = unique_objects(catalog, repository)?;

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
    repository: &Repository,
) -> anyhow::Result<BTreeMap<ContentHash, Vec<u8>>> {
    let mut objects = BTreeMap::new();
    for record in &catalog.assets {
        let source = repository
            .assets
            .get(&record.id)
            .with_context(|| format!("missing object source for `{}`", record.id))?;
        objects
            .entry(record.content_hash)
            .or_insert_with(|| source.source_bytes.clone());
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
        MOVEFILE_REPLACE_EXISTING, MOVEFILE_WRITE_THROUGH, MoveFileExW,
    };

    let candidate = candidate
        .as_os_str()
        .encode_wide()
        .chain(core::iter::once(0))
        .collect::<Vec<_>>();
    let output = output
        .as_os_str()
        .encode_wide()
        .chain(core::iter::once(0))
        .collect::<Vec<_>>();
    let result = unsafe {
        MoveFileExW(
            candidate.as_ptr(),
            output.as_ptr(),
            MOVEFILE_REPLACE_EXISTING | MOVEFILE_WRITE_THROUGH,
        )
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
    use std::collections::BTreeSet;
    use std::fs;
    use std::str::FromStr;

    use anyhow::Context;
    use blackflower_assets::{
        AssetCatalog, AssetId, AssetPackage, AssetSigningKey, AssetStore, AssetTrustStore, Bytes,
        ContentHash, Error, PackageName, sign_package,
    };
    use tempfile::TempDir;

    use crate::manifest::Repository;

    use super::{CookRequest, Pipeline, build_catalog, toolchain_identity, write_package};

    const TEST_SIGNING_SECRET: [u8; 32] = [0x42; 32];

    #[test]
    fn cooks_deterministically_and_reuses_the_logical_name() -> anyhow::Result<()> {
        let fixture = Fixture::new()?;
        fixture.asset("fixtures/example", "shared", "example.bin", b"first")?;
        let request = fixture.request("pak000", &["fixtures/example"])?;
        let first = fixture.pipeline.cook(&request)?;
        assert_eq!(
            first.package_hash.to_string(),
            "b2ee50fc0b96cd9bc7aa5b391ec95837d4da61bd5da882aabee83db289510030"
        );
        let first_bytes = fs::read(&first.path)?;
        let second = fixture.pipeline.cook(&request)?;
        assert_eq!(first.package_hash, second.package_hash);
        assert_eq!(first_bytes, fs::read(&second.path)?);
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
            profile: "desktop-universal".to_owned(),
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
        let repository = Repository::load(&fixture.source)?;
        let directory = fixture.package_dir();
        fs::create_dir_all(&directory)?;
        let path = directory.join("pak000.squashfs");
        write_package(&path, &catalog, &repository)?;

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
            profile: "desktop-universal".to_owned(),
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
        source: std::path::PathBuf,
        target: std::path::PathBuf,
        pipeline: Pipeline,
    }

    impl Fixture {
        fn new() -> anyhow::Result<Self> {
            let temp = TempDir::new()?;
            let source = temp.path().join("assets/source");
            let target = temp.path().join("target");
            fs::create_dir_all(&source)?;
            let pipeline = Pipeline::new(source.clone(), target.clone());
            Ok(Self {
                _temp: temp,
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
                profile: "desktop-universal".to_owned(),
                package: PackageName::from_str(package)?,
                signing_key: Self::signing_key(),
            })
        }

        fn package_dir(&self) -> std::path::PathBuf {
            self.target.join("assets/packages/desktop-universal")
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

        fn selected_catalog(&self, selected: &[&str]) -> anyhow::Result<AssetCatalog> {
            let repository = Repository::load(&self.source)?;
            let selected = selected
                .iter()
                .map(|id| AssetId::from_str(id))
                .collect::<Result<BTreeSet<_>, _>>()?;
            let toolchain = toolchain_identity();
            let toolchain_bytes = serde_json::to_vec(&toolchain)?;
            let hashes = repository.recipe_hashes("desktop-universal", &toolchain_bytes)?;
            build_catalog(
                &repository,
                &selected,
                &hashes,
                "desktop-universal",
                toolchain,
            )
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
            let repository = Repository::load(&self.source)?;
            let directory = self.package_dir();
            fs::create_dir_all(&directory)?;
            let name = PackageName::from_str(package)?;
            let path = directory.join(name.file_name());
            write_package(&path, catalog, &repository)?;
            let _payload_hash = sign_package(&path, &Self::signing_key())?;
            Ok(path)
        }
    }
}
