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
            AssetSource::Mesh(_) => AssetKind::Mesh,
            AssetSource::Model(_) => AssetKind::Model,
            AssetSource::Volume(_) => AssetKind::Volume,
            AssetSource::Skeleton(_) => AssetKind::Skeleton,
            AssetSource::Animation(_) => AssetKind::AnimationClip,
        }
    }

    pub(crate) fn dependencies(&self) -> Vec<AssetId> {
        match &self.source {
            AssetSource::Animation(manifest) => vec![manifest.skeleton.clone()],
            AssetSource::Model(manifest) => manifest
                .attachments
                .iter()
                .map(|attachment| attachment.asset.clone())
                .collect::<BTreeSet<_>>()
                .into_iter()
                .collect(),
            AssetSource::Blob(_)
            | AssetSource::Luau(_)
            | AssetSource::Shader(_)
            | AssetSource::Texture(_)
            | AssetSource::Mesh(_)
            | AssetSource::Volume(_)
            | AssetSource::Skeleton(_) => Vec::new(),
        }
    }
}

#[derive(Debug, Clone)]
pub(crate) enum AssetSource {
    Blob(BlobManifest),
    Luau(LuauManifest),
    Shader(ShaderManifest),
    Texture(TextureManifest),
    Mesh(MeshManifest),
    Model(ModelManifest),
    Volume(VolumeManifest),
    Skeleton(SkeletonManifest),
    Animation(AnimationManifest),
}

impl AssetSource {
    fn source(&self) -> &Path {
        match self {
            Self::Blob(manifest) => &manifest.source,
            Self::Luau(manifest) => &manifest.source,
            Self::Shader(manifest) => &manifest.source,
            Self::Texture(manifest) => &manifest.source,
            Self::Mesh(manifest) => &manifest.source,
            Self::Model(manifest) => &manifest.source,
            Self::Volume(manifest) => &manifest.source,
            Self::Skeleton(manifest) => &manifest.source,
            Self::Animation(manifest) => &manifest.source,
        }
    }
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

#[derive(Debug, Clone, Deserialize)]
#[serde(deny_unknown_fields)]
pub(crate) struct MeshManifest {
    pub(crate) source: PathBuf,
    pub(crate) mesh: String,
}

#[derive(Debug, Clone, Deserialize)]
#[serde(deny_unknown_fields)]
pub(crate) struct ModelManifest {
    pub(crate) source: PathBuf,
    pub(crate) scene: String,
    #[serde(default)]
    pub(crate) attachments: Vec<ModelAttachmentManifest>,
}

#[derive(Debug, Clone, Deserialize)]
#[serde(deny_unknown_fields)]
pub(crate) struct ModelAttachmentManifest {
    pub(crate) node: String,
    pub(crate) asset: AssetId,
}

#[derive(Debug, Clone, Deserialize)]
#[serde(deny_unknown_fields)]
pub(crate) struct VolumeManifest {
    pub(crate) source: PathBuf,
    pub(crate) grids: Vec<String>,
}

#[derive(Debug, Clone, Deserialize)]
#[serde(deny_unknown_fields)]
pub(crate) struct SkeletonManifest {
    pub(crate) source: PathBuf,
    pub(crate) skin: String,
}

#[derive(Debug, Clone, Deserialize)]
#[serde(deny_unknown_fields)]
pub(crate) struct AnimationManifest {
    pub(crate) source: PathBuf,
    pub(crate) clip: String,
    pub(crate) skeleton: AssetId,
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
    mesh: Option<MeshManifest>,
    model: Option<ModelManifest>,
    volume: Option<VolumeManifest>,
    skeleton: Option<SkeletonManifest>,
    animation: Option<AnimationManifest>,
}

struct SourceSections {
    blob: Option<BlobManifest>,
    luau: Option<LuauManifest>,
    shader: Option<ShaderManifest>,
    texture: Option<TextureManifest>,
    mesh: Option<MeshManifest>,
    model: Option<ModelManifest>,
    volume: Option<VolumeManifest>,
    skeleton: Option<SkeletonManifest>,
    animation: Option<AnimationManifest>,
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
    pub(crate) source_path: PathBuf,
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
            if is_asset_manifest_path(&path) {
                load_asset(&canonical_root, &path, &mut assets)?;
            } else if path.file_name().is_some_and(|name| name == "package.toml") {
                load_package(&canonical_root, &path, &mut packages)?;
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
        let mut selected = package.assets.iter().cloned().collect::<BTreeSet<_>>();
        loop {
            let dependencies = selected
                .iter()
                .filter_map(|id| self.assets.get(id))
                .flat_map(|asset| asset.manifest.dependencies())
                .filter(|dependency| !selected.contains(dependency))
                .collect::<Vec<_>>();
            if dependencies.is_empty() {
                break;
            }
            selected.extend(dependencies);
        }
        Ok(selected)
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
        for (id, asset) in &self.assets {
            for dependency in asset.manifest.dependencies() {
                let target = self.assets.get(&dependency).with_context(|| {
                    format!("asset `{id}` references missing dependency `{dependency}`")
                })?;
                match &asset.manifest.source {
                    AssetSource::Animation(_) => {
                        if matches!(target.manifest.source, AssetSource::Skeleton(_)) {
                            continue;
                        }
                        bail!("animation asset `{id}` dependency `{dependency}` is not a skeleton");
                    }
                    AssetSource::Model(_) => {
                        if matches!(
                            target.manifest.source,
                            AssetSource::Mesh(_) | AssetSource::Volume(_)
                        ) {
                            continue;
                        }
                        bail!(
                            "model asset `{id}` attachment `{dependency}` is not a mesh or volume"
                        );
                    }
                    AssetSource::Blob(_)
                    | AssetSource::Luau(_)
                    | AssetSource::Shader(_)
                    | AssetSource::Texture(_)
                    | AssetSource::Mesh(_)
                    | AssetSource::Volume(_)
                    | AssetSource::Skeleton(_) => {}
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
    is_asset_manifest_path(path)
        || path
            .file_name()
            .is_some_and(|file_name| file_name == "package.toml")
}

fn is_asset_manifest_path(path: &Path) -> bool {
    path.file_name()
        .and_then(|value| value.to_str())
        .is_some_and(|file_name| file_name == "asset.toml" || file_name.ends_with(".asset.toml"))
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
    let sections = SourceSections {
        blob: file.blob,
        luau: file.luau,
        shader: file.shader,
        texture: file.texture,
        mesh: file.mesh,
        model: file.model,
        volume: file.volume,
        skeleton: file.skeleton,
        animation: file.animation,
    };
    let source = asset_source(file.kind, file.audience, sections, path)?;
    let source_path = source.source();
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
        source_path,
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
    sections: SourceSections,
    path: &Path,
) -> anyhow::Result<AssetSource> {
    if sections.count() != 1 {
        bail!(
            "asset manifest `{}` must contain exactly one source section",
            path.display()
        );
    }

    let source = match kind {
        AssetKind::Blob => AssetSource::Blob(required_section(sections.blob, "blob", path)?),
        AssetKind::LuauBytecode => {
            AssetSource::Luau(required_section(sections.luau, "luau", path)?)
        }
        AssetKind::ShaderModule => {
            let shader = required_section(sections.shader, "shader", path)?;
            validate_shader_manifest(&shader, audience, path)?;
            AssetSource::Shader(shader)
        }
        AssetKind::Texture2d => {
            let texture = required_section(sections.texture, "texture", path)?;
            validate_texture_manifest(&texture, audience, path)?;
            AssetSource::Texture(texture)
        }
        AssetKind::Mesh => {
            let mesh = required_section(sections.mesh, "mesh", path)?;
            validate_mesh_manifest(&mesh, audience, path)?;
            AssetSource::Mesh(mesh)
        }
        AssetKind::Model => {
            let mut model = required_section(sections.model, "model", path)?;
            validate_model_manifest(&mut model, audience, path)?;
            AssetSource::Model(model)
        }
        AssetKind::Volume => {
            let mut volume = required_section(sections.volume, "volume", path)?;
            validate_volume_manifest(&mut volume, audience, path)?;
            AssetSource::Volume(volume)
        }
        AssetKind::Skeleton => {
            let skeleton = required_section(sections.skeleton, "skeleton", path)?;
            validate_skeleton_manifest(&skeleton, audience, path)?;
            AssetSource::Skeleton(skeleton)
        }
        AssetKind::AnimationClip => {
            let animation = required_section(sections.animation, "animation", path)?;
            validate_animation_manifest(&animation, audience, path)?;
            AssetSource::Animation(animation)
        }
        _ => bail!("unsupported asset kind in `{}`", path.display()),
    };
    Ok(source)
}

impl SourceSections {
    fn count(&self) -> usize {
        usize::from(self.blob.is_some())
            + usize::from(self.luau.is_some())
            + usize::from(self.shader.is_some())
            + usize::from(self.texture.is_some())
            + usize::from(self.mesh.is_some())
            + usize::from(self.model.is_some())
            + usize::from(self.volume.is_some())
            + usize::from(self.skeleton.is_some())
            + usize::from(self.animation.is_some())
    }
}

fn required_section<T>(section: Option<T>, name: &str, path: &Path) -> anyhow::Result<T> {
    section.with_context(|| {
        format!(
            "{name} asset manifest `{}` requires a `[{name}]` section",
            path.display()
        )
    })
}

fn validate_skeleton_manifest(
    skeleton: &SkeletonManifest,
    audience: AssetAudience,
    path: &Path,
) -> anyhow::Result<()> {
    validate_animation_audience(audience, path)?;
    validate_gltf_source(&skeleton.source, "skeleton", path)?;
    validate_selection_name(&skeleton.skin, "skin", path)
}

fn validate_animation_manifest(
    animation: &AnimationManifest,
    audience: AssetAudience,
    path: &Path,
) -> anyhow::Result<()> {
    validate_animation_audience(audience, path)?;
    validate_gltf_source(&animation.source, "animation", path)?;
    validate_selection_name(&animation.clip, "clip", path)
}

fn validate_animation_audience(audience: AssetAudience, path: &Path) -> anyhow::Result<()> {
    if audience != AssetAudience::Presentation {
        bail!(
            "animation asset manifest `{}` must use audience `presentation`",
            path.display()
        );
    }
    Ok(())
}

fn validate_gltf_source(source: &Path, label: &str, path: &Path) -> anyhow::Result<()> {
    let extension = source
        .extension()
        .and_then(|value| value.to_str())
        .with_context(|| format!("{label} source must have a UTF-8 extension"))?;
    if !extension.eq_ignore_ascii_case("gltf") && !extension.eq_ignore_ascii_case("glb") {
        bail!(
            "{label} source in `{}` must use glTF or GLB",
            path.display()
        );
    }
    Ok(())
}

fn validate_selection_name(value: &str, label: &str, path: &Path) -> anyhow::Result<()> {
    if value.is_empty()
        || value.trim() != value
        || value.chars().any(char::is_control)
        || value.contains(['*', '?'])
    {
        bail!(
            "{label} name in `{}` must be non-empty, unpadded, and contain no wildcards or control characters",
            path.display()
        );
    }
    Ok(())
}

fn validate_mesh_manifest(
    mesh: &MeshManifest,
    audience: AssetAudience,
    path: &Path,
) -> anyhow::Result<()> {
    if audience != AssetAudience::Presentation {
        bail!(
            "mesh asset manifest `{}` must use audience `presentation`",
            path.display()
        );
    }
    if mesh.mesh.is_empty() || mesh.mesh.chars().any(char::is_control) {
        bail!(
            "mesh name in `{}` must be non-empty and contain no control characters",
            path.display()
        );
    }
    validate_gltf_source(&mesh.source, "mesh", path)
}

fn validate_model_manifest(
    model: &mut ModelManifest,
    audience: AssetAudience,
    path: &Path,
) -> anyhow::Result<()> {
    if audience != AssetAudience::Presentation {
        bail!(
            "model asset manifest `{}` must use audience `presentation`",
            path.display()
        );
    }
    validate_gltf_source(&model.source, "model", path)?;
    validate_selection_name(&model.scene, "scene", path)?;
    for attachment in &model.attachments {
        validate_selection_name(&attachment.node, "attachment node", path)?;
    }
    model
        .attachments
        .sort_by(|left, right| (&left.node, &left.asset).cmp(&(&right.node, &right.asset)));
    for pair in model.attachments.windows(2) {
        if pair[0].node == pair[1].node && pair[0].asset == pair[1].asset {
            bail!(
                "model manifest `{}` repeats attachment `{}` on node `{}`",
                path.display(),
                pair[0].asset,
                pair[0].node
            );
        }
    }
    Ok(())
}

fn validate_volume_manifest(
    volume: &mut VolumeManifest,
    audience: AssetAudience,
    path: &Path,
) -> anyhow::Result<()> {
    if audience != AssetAudience::Presentation {
        bail!(
            "volume asset manifest `{}` must use audience `presentation`",
            path.display()
        );
    }
    let extension = volume
        .source
        .extension()
        .and_then(|value| value.to_str())
        .context("volume source must have a UTF-8 extension")?;
    if !extension.eq_ignore_ascii_case("vdb") {
        bail!(
            "volume source in `{}` must use OpenVDB `.vdb`",
            path.display()
        );
    }
    if volume.grids.is_empty() {
        bail!(
            "volume manifest `{}` must select at least one grid",
            path.display()
        );
    }
    for grid in &volume.grids {
        validate_selection_name(grid, "grid", path)?;
    }
    volume.grids.sort();
    reject_duplicates(&volume.grids, "grid", path)
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

fn reject_duplicates<T>(values: &[T], label: &str, path: &Path) -> anyhow::Result<()>
where
    T: std::fmt::Display + PartialEq,
{
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
