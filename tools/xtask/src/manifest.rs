use std::collections::{BTreeMap, BTreeSet};
use std::fs;
use std::path::{Component, Path, PathBuf};
use std::str::FromStr;

use anyhow::{Context, bail};
use blackflower_assets::{AssetAudience, AssetId, AssetKind, ContentHash, PackageName};
use blackflower_shader_compiler::ShaderStage;
use serde::{Deserialize, Serialize};

pub(crate) const SOURCE_SCHEMA: u32 = 1;

#[derive(Debug, Clone)]
pub(crate) struct AssetManifest {
    pub(crate) schema: u32,
    pub(crate) id: AssetId,
    pub(crate) audience: AssetAudience,
    pub(crate) source: AssetSource,
}

impl AssetManifest {
    pub(crate) const fn kind(&self) -> AssetKind {
        match self.source {
            AssetSource::Blob(_) => AssetKind::Blob,
            AssetSource::Luau(_) => AssetKind::LuauBytecode,
            AssetSource::Shader(_) => AssetKind::ShaderModule,
            AssetSource::Texture(_) => AssetKind::Texture2d,
        }
    }
}

#[derive(Debug, Clone)]
pub(crate) enum AssetSource {
    Blob(BlobManifest),
    Luau(LuauManifest),
    Shader(ShaderManifest),
    Texture(TextureManifest),
}

#[derive(Debug, Clone, Deserialize)]
#[serde(deny_unknown_fields)]
pub(crate) struct BlobManifest {
    pub(crate) source: PathBuf,
}

#[derive(Debug, Clone, Deserialize)]
#[serde(deny_unknown_fields)]
pub(crate) struct LuauManifest {
    pub(crate) source: PathBuf,
}

#[derive(Debug, Clone, Deserialize)]
#[serde(deny_unknown_fields)]
pub(crate) struct ShaderManifest {
    pub(crate) source: PathBuf,
    pub(crate) entry_point: String,
    pub(crate) stage: ShaderStageManifest,
}

#[derive(Debug, Clone, Deserialize)]
#[serde(deny_unknown_fields)]
pub(crate) struct TextureManifest {
    pub(crate) source: PathBuf,
    pub(crate) semantic: TextureSemanticManifest,
}

#[derive(Debug, Clone, Copy, Serialize, Deserialize)]
#[serde(rename_all = "snake_case")]
pub(crate) enum TextureSemanticManifest {
    ColorSrgb,
    NormalLinear,
    DataLinear,
    HdrLinear,
}

#[derive(Debug, Clone, Copy, Serialize, Deserialize)]
#[serde(rename_all = "snake_case")]
pub(crate) enum ShaderStageManifest {
    Vertex,
    Fragment,
    Compute,
}

impl From<ShaderStageManifest> for ShaderStage {
    fn from(value: ShaderStageManifest) -> Self {
        match value {
            ShaderStageManifest::Vertex => Self::Vertex,
            ShaderStageManifest::Fragment => Self::Fragment,
            ShaderStageManifest::Compute => Self::Compute,
        }
    }
}

#[derive(Debug, Clone, Deserialize)]
#[serde(deny_unknown_fields)]
struct AssetManifestFile {
    schema: u32,
    id: AssetId,
    kind: AssetKind,
    audience: AssetAudience,
    blob: Option<BlobManifest>,
    luau: Option<LuauManifest>,
    shader: Option<ShaderManifest>,
    texture: Option<TextureManifest>,
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
    pub(crate) source_hash: ContentHash,
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
    let file: AssetManifestFile =
        toml::from_str(&text).with_context(|| format!("invalid `{}`", path.display()))?;
    validate_schema(file.schema, path)?;
    let source = asset_source(
        file.kind,
        file.audience,
        file.blob,
        file.luau,
        file.shader,
        file.texture,
        path,
    )?;
    let source_path = match &source {
        AssetSource::Blob(manifest) => &manifest.source,
        AssetSource::Luau(manifest) => &manifest.source,
        AssetSource::Shader(manifest) => &manifest.source,
        AssetSource::Texture(manifest) => &manifest.source,
    };
    let source_relative = portable_relative_path(source_path)
        .with_context(|| format!("invalid source path in `{}`", path.display()))?;
    let source_path = resolve_source(source_root, path, source_path)?;
    let source_bytes = fs::read(&source_path)
        .with_context(|| format!("failed to read `{}`", source_path.display()))?;
    let source_hash = ContentHash::hash_bytes(&source_bytes);
    let manifest = AssetManifest {
        schema: file.schema,
        id: file.id,
        audience: file.audience,
        source,
    };
    let id = manifest.id.clone();
    let loaded = LoadedAsset {
        manifest,
        source_relative,
        source_bytes,
        source_hash,
    };
    if assets.insert(id.clone(), loaded).is_some() {
        bail!("duplicate asset ID `{id}`");
    }
    Ok(())
}

fn asset_source(
    kind: AssetKind,
    audience: AssetAudience,
    blob: Option<BlobManifest>,
    luau: Option<LuauManifest>,
    shader: Option<ShaderManifest>,
    texture: Option<TextureManifest>,
    path: &Path,
) -> anyhow::Result<AssetSource> {
    let source_sections = usize::from(blob.is_some())
        + usize::from(luau.is_some())
        + usize::from(shader.is_some())
        + usize::from(texture.is_some());
    if source_sections != 1 {
        bail!(
            "asset manifest `{}` must contain exactly one source section",
            path.display()
        );
    }

    let source = match kind {
        AssetKind::Blob => AssetSource::Blob(blob.with_context(|| {
            format!(
                "blob asset manifest `{}` requires a `[blob]` section",
                path.display()
            )
        })?),
        AssetKind::LuauBytecode => AssetSource::Luau(luau.with_context(|| {
            format!(
                "Luau bytecode manifest `{}` requires a `[luau]` section",
                path.display()
            )
        })?),
        AssetKind::ShaderModule => {
            let shader = shader.with_context(|| {
                format!(
                    "shader module manifest `{}` requires a `[shader]` section",
                    path.display()
                )
            })?;
            validate_shader_manifest(&shader, audience, path)?;
            AssetSource::Shader(shader)
        }
        AssetKind::Texture2d => {
            let texture = texture.with_context(|| {
                format!(
                    "texture manifest `{}` requires a `[texture]` section",
                    path.display()
                )
            })?;
            validate_texture_manifest(&texture, audience, path)?;
            AssetSource::Texture(texture)
        }
        _ => bail!("unsupported asset kind in `{}`", path.display()),
    };
    Ok(source)
}

fn validate_texture_manifest(
    texture: &TextureManifest,
    audience: AssetAudience,
    path: &Path,
) -> anyhow::Result<()> {
    if audience != AssetAudience::Presentation {
        bail!(
            "texture manifest `{}` must use audience `presentation`",
            path.display()
        );
    }
    let extension = texture
        .source
        .extension()
        .and_then(|value| value.to_str())
        .context("texture source must have a UTF-8 extension")?;
    let valid_extension = match texture.semantic {
        TextureSemanticManifest::HdrLinear => extension.eq_ignore_ascii_case("exr"),
        TextureSemanticManifest::ColorSrgb
        | TextureSemanticManifest::NormalLinear
        | TextureSemanticManifest::DataLinear => extension.eq_ignore_ascii_case("png"),
    };
    if !valid_extension {
        bail!(
            "texture source in `{}` must use PNG for LDR semantics or EXR for HDR",
            path.display()
        );
    }
    Ok(())
}

fn validate_shader_manifest(
    shader: &ShaderManifest,
    audience: AssetAudience,
    path: &Path,
) -> anyhow::Result<()> {
    if audience != AssetAudience::Presentation {
        bail!(
            "shader module manifest `{}` must use audience `presentation`",
            path.display()
        );
    }
    let mut characters = shader.entry_point.chars();
    let valid_first = characters
        .next()
        .is_some_and(|value| value == '_' || value.is_ascii_alphabetic());
    if !valid_first || !characters.all(|value| value == '_' || value.is_ascii_alphanumeric()) {
        bail!(
            "shader entry point in `{}` must be a portable identifier",
            path.display()
        );
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
