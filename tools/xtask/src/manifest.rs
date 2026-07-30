use std::collections::{BTreeMap, BTreeSet};
use std::fs;
use std::path::{Component, Path, PathBuf};
use std::str::FromStr;

use anyhow::{Context, bail};
use blackflower_assets::{AssetAudience, AssetId, AssetKind, ContentHash, PackageName, RecipeHash};
use serde::Deserialize;

pub(crate) const SOURCE_SCHEMA: u32 = 1;

#[derive(Debug, Clone, Deserialize)]
#[serde(deny_unknown_fields)]
pub(crate) struct AssetManifest {
    pub(crate) schema: u32,
    pub(crate) id: AssetId,
    pub(crate) kind: AssetKind,
    pub(crate) audience: AssetAudience,
    pub(crate) blob: BlobManifest,
}

#[derive(Debug, Clone, Deserialize)]
#[serde(deny_unknown_fields)]
pub(crate) struct BlobManifest {
    pub(crate) source: PathBuf,
}

#[derive(Debug, Clone, Deserialize)]
#[serde(deny_unknown_fields)]
struct PackageManifestFile {
    pub(crate) schema: u32,
    pub(crate) assets: Vec<AssetId>,
}

#[derive(Debug, Clone)]
pub(crate) struct PackageManifest {
    pub(crate) name: PackageName,
    pub(crate) assets: Vec<AssetId>,
}

#[derive(Debug)]
pub(crate) struct LoadedAsset {
    pub(crate) manifest: AssetManifest,
    pub(crate) source_relative: String,
    pub(crate) source_bytes: Vec<u8>,
    pub(crate) content_hash: ContentHash,
}

#[derive(Debug)]
pub(crate) struct Repository {
    pub(crate) assets: BTreeMap<AssetId, LoadedAsset>,
    pub(crate) packages: BTreeMap<PackageName, PackageManifest>,
}

impl Repository {
    pub(crate) fn load(source_root: &Path) -> anyhow::Result<Self> {
        let canonical_root = source_root.canonicalize().with_context(|| {
            format!(
                "asset source root `{}` does not exist",
                source_root.display()
            )
        })?;
        let mut manifest_paths = Vec::new();
        collect_manifest_paths(&canonical_root, &mut manifest_paths)?;
        manifest_paths.sort();

        let mut assets = BTreeMap::new();
        let mut packages = BTreeMap::new();
        for path in manifest_paths {
            match path.file_name().and_then(|value| value.to_str()) {
                Some("asset.toml") => load_asset(&canonical_root, &path, &mut assets)?,
                Some("package.toml") => {
                    load_package(&canonical_root, &path, &mut packages)?;
                }
                _ => {}
            }
        }
        let repository = Self { assets, packages };
        repository.validate_graph()?;
        Ok(repository)
    }

    pub(crate) fn selected_assets(
        &self,
        package_name: &PackageName,
    ) -> anyhow::Result<BTreeSet<AssetId>> {
        let package = self
            .packages
            .get(package_name)
            .with_context(|| format!("package `{package_name}` has no package.toml"))?;
        Ok(package.assets.iter().cloned().collect())
    }

    pub(crate) fn recipe_hashes(
        &self,
        profile: &str,
        toolchain_bytes: &[u8],
    ) -> anyhow::Result<BTreeMap<AssetId, RecipeHash>> {
        let mut hashes = BTreeMap::new();
        for id in self.assets.keys() {
            let hash = recipe_hash(self, id, profile, toolchain_bytes)?;
            hashes.insert(id.clone(), hash);
        }
        Ok(hashes)
    }

    fn validate_graph(&self) -> anyhow::Result<()> {
        for package in self.packages.values() {
            for asset in &package.assets {
                if !self.assets.contains_key(asset) {
                    bail!(
                        "package `{}` references missing asset `{asset}`",
                        package.name,
                    );
                }
            }
        }
        Ok(())
    }
}

fn collect_manifest_paths(root: &Path, output: &mut Vec<PathBuf>) -> anyhow::Result<()> {
    let entries =
        fs::read_dir(root).with_context(|| format!("failed to read `{}`", root.display()))?;
    let mut paths = entries
        .map(|entry| entry.map(|value| value.path()))
        .collect::<Result<Vec<_>, _>>()
        .with_context(|| format!("failed to enumerate `{}`", root.display()))?;
    paths.sort();
    for path in paths {
        let metadata = fs::symlink_metadata(&path)
            .with_context(|| format!("failed to inspect `{}`", path.display()))?;
        if metadata.file_type().is_symlink() {
            if is_manifest_path(&path) {
                bail!("manifest `{}` cannot be a symlink", path.display());
            }
            continue;
        }
        if metadata.is_dir() {
            collect_manifest_paths(&path, output)?;
        } else if metadata.is_file() && is_manifest_path(&path) {
            output.push(path);
        }
    }
    Ok(())
}

fn is_manifest_path(path: &Path) -> bool {
    matches!(
        path.file_name().and_then(|value| value.to_str()),
        Some("asset.toml" | "package.toml")
    )
}

fn load_asset(
    source_root: &Path,
    path: &Path,
    assets: &mut BTreeMap<AssetId, LoadedAsset>,
) -> anyhow::Result<()> {
    let text =
        fs::read_to_string(path).with_context(|| format!("failed to read `{}`", path.display()))?;
    let manifest: AssetManifest =
        toml::from_str(&text).with_context(|| format!("invalid `{}`", path.display()))?;
    validate_schema(manifest.schema, path)?;
    let source_relative = portable_relative_path(&manifest.blob.source)
        .with_context(|| format!("invalid source path in `{}`", path.display()))?;
    let source_path = resolve_source(source_root, path, &manifest.blob.source)?;
    let source_bytes = fs::read(&source_path)
        .with_context(|| format!("failed to read `{}`", source_path.display()))?;
    let content_hash = ContentHash::hash_bytes(&source_bytes);
    let id = manifest.id.clone();
    let loaded = LoadedAsset {
        manifest,
        source_relative,
        source_bytes,
        content_hash,
    };
    if assets.insert(id.clone(), loaded).is_some() {
        bail!("duplicate asset ID `{id}`");
    }
    Ok(())
}

fn load_package(
    source_root: &Path,
    path: &Path,
    packages: &mut BTreeMap<PackageName, PackageManifest>,
) -> anyhow::Result<()> {
    let relative = path
        .strip_prefix(source_root)
        .with_context(|| format!("package manifest `{}` escapes source root", path.display()))?;
    let mut components = relative.components();
    let package_name = match (
        components.next(),
        components.next(),
        components.next(),
        components.next(),
    ) {
        (
            Some(Component::Normal(packages_directory)),
            Some(Component::Normal(package_name)),
            Some(Component::Normal(file_name)),
            None,
        ) if packages_directory == "packages" && file_name == "package.toml" => package_name,
        _ => {
            bail!(
                "package manifest `{}` must be at `packages/<logical-name>/package.toml`",
                path.display()
            );
        }
    };
    let package_name = package_name
        .to_str()
        .context("package directory name must be UTF-8")
        .and_then(|name| PackageName::from_str(name).map_err(anyhow::Error::from))?;
    let text =
        fs::read_to_string(path).with_context(|| format!("failed to read `{}`", path.display()))?;
    let mut file: PackageManifestFile =
        toml::from_str(&text).with_context(|| format!("invalid `{}`", path.display()))?;
    validate_schema(file.schema, path)?;
    file.assets.sort();
    reject_duplicates(&file.assets, "asset", path)?;
    let manifest = PackageManifest {
        name: package_name.clone(),
        assets: file.assets,
    };
    if packages.insert(package_name.clone(), manifest).is_some() {
        bail!("duplicate package manifest for `{package_name}`");
    }
    Ok(())
}

fn validate_schema(schema: u32, path: &Path) -> anyhow::Result<()> {
    if schema != SOURCE_SCHEMA {
        bail!("unsupported schema {schema} in `{}`", path.display());
    }
    Ok(())
}

fn reject_duplicates(values: &[AssetId], label: &str, path: &Path) -> anyhow::Result<()> {
    for pair in values.windows(2) {
        if pair[0] == pair[1] {
            bail!("duplicate {label} `{}` in `{}`", pair[0], path.display());
        }
    }
    Ok(())
}

fn resolve_source(source_root: &Path, manifest: &Path, source: &Path) -> anyhow::Result<PathBuf> {
    if source.as_os_str().is_empty() || source.is_absolute() {
        bail!("source path must be a non-empty relative path");
    }
    if source
        .components()
        .any(|component| !matches!(component, Component::Normal(_)))
    {
        bail!("source path cannot contain traversal or platform prefixes");
    }
    let parent = manifest
        .parent()
        .with_context(|| format!("manifest `{}` has no parent", manifest.display()))?;
    let candidate = parent.join(source);
    let canonical = candidate
        .canonicalize()
        .with_context(|| format!("source `{}` does not exist", candidate.display()))?;
    if !canonical.starts_with(source_root) {
        bail!(
            "source `{}` escapes the asset source root",
            candidate.display()
        );
    }
    if !canonical.is_file() {
        bail!("source `{}` is not a regular file", canonical.display());
    }
    Ok(canonical)
}

fn portable_relative_path(path: &Path) -> anyhow::Result<String> {
    let mut parts = Vec::new();
    for component in path.components() {
        let Component::Normal(value) = component else {
            bail!("path must contain only normal relative components");
        };
        let value = value.to_str().context("path must be UTF-8")?;
        parts.push(value);
    }
    if parts.is_empty() {
        bail!("path is empty");
    }
    Ok(parts.join("/"))
}

fn recipe_hash(
    repository: &Repository,
    id: &AssetId,
    profile: &str,
    toolchain_bytes: &[u8],
) -> anyhow::Result<RecipeHash> {
    let asset = repository
        .assets
        .get(id)
        .with_context(|| format!("unknown asset `{id}`"))?;
    let mut hasher = CanonicalHasher::new(b"blackflower.asset-recipe.v1");
    hasher.u32(asset.manifest.schema);
    hasher.text(profile);
    hasher.text(asset.manifest.id.as_str());
    hasher.serializable(&asset.manifest.kind)?;
    hasher.serializable(&asset.manifest.audience)?;
    hasher.text(&asset.source_relative);
    hasher.bytes(asset.content_hash.as_bytes());
    hasher.bytes(toolchain_bytes);
    Ok(RecipeHash::from_bytes(*hasher.finish().as_bytes()))
}

struct CanonicalHasher(blake3::Hasher);

impl CanonicalHasher {
    fn new(domain: &[u8]) -> Self {
        let mut value = Self(blake3::Hasher::new());
        value.bytes(domain);
        value
    }

    fn bytes(&mut self, bytes: &[u8]) {
        self.u64(u64::try_from(bytes.len()).unwrap_or(u64::MAX));
        self.0.update(bytes);
    }

    fn text(&mut self, value: &str) {
        self.bytes(value.as_bytes());
    }

    fn u32(&mut self, value: u32) {
        self.0.update(&value.to_le_bytes());
    }

    fn u64(&mut self, value: u64) {
        self.0.update(&value.to_le_bytes());
    }

    fn serializable(&mut self, value: &impl serde::Serialize) -> anyhow::Result<()> {
        self.bytes(&serde_json::to_vec(value)?);
        Ok(())
    }

    fn finish(self) -> blake3::Hash {
        self.0.finalize()
    }
}
