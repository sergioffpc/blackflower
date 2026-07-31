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
            AssetSource::Navigation(_) => AssetKind::NavigationMesh,
            AssetSource::AudioClip(_) => AssetKind::AudioClip,
            AssetSource::AudioStream(_) => AssetKind::AudioStream,
            AssetSource::SoundEvent(_) => AssetKind::SoundEvent,
            AssetSource::AcousticScene(_) => AssetKind::AcousticScene,
            AssetSource::AcousticProbes(_) => AssetKind::AcousticProbeBatch,
            AssetSource::Acoustic(_) => AssetKind::AcousticEnvironment,
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
            AssetSource::SoundEvent(manifest) => vec![manifest.media.clone()],
            AssetSource::AcousticProbes(manifest) => vec![manifest.scene.clone()],
            AssetSource::Acoustic(manifest) => manifest
                .zones
                .iter()
                .flat_map(|zone| [zone.scene.clone(), zone.probes.clone()])
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
            AssetSource::Navigation(_)
            | AssetSource::AudioClip(_)
            | AssetSource::AudioStream(_)
            | AssetSource::AcousticScene(_) => Vec::new(),
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
    Navigation(NavigationManifest),
    AudioClip(AudioClipManifest),
    AudioStream(AudioStreamManifest),
    SoundEvent(SoundEventManifest),
    AcousticScene(AcousticSceneManifest),
    AcousticProbes(AcousticProbesManifest),
    Acoustic(AcousticManifest),
}

impl AssetSource {
    fn source(&self) -> Option<&Path> {
        match self {
            Self::Blob(manifest) => Some(&manifest.source),
            Self::Luau(manifest) => Some(&manifest.source),
            Self::Shader(manifest) => Some(&manifest.source),
            Self::Texture(manifest) => Some(&manifest.source),
            Self::Mesh(manifest) => Some(&manifest.source),
            Self::Model(manifest) => Some(&manifest.source),
            Self::Volume(manifest) => Some(&manifest.source),
            Self::Skeleton(manifest) => Some(&manifest.source),
            Self::Animation(manifest) => Some(&manifest.source),
            Self::Navigation(manifest) => Some(&manifest.source),
            Self::AudioClip(manifest) => Some(&manifest.source),
            Self::AudioStream(manifest) => Some(&manifest.source),
            Self::SoundEvent(_) => None,
            Self::AcousticScene(manifest) => Some(&manifest.source),
            Self::AcousticProbes(manifest) => Some(&manifest.source),
            Self::Acoustic(manifest) => Some(&manifest.source),
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

#[derive(Debug, Clone, Serialize, Deserialize)]
#[serde(deny_unknown_fields)]
pub(crate) struct NavigationManifest {
    pub(crate) source: PathBuf,
    pub(crate) profile_id: String,
    pub(crate) agent: NavigationAgentManifest,
    pub(crate) build: NavigationBuildManifest,
    pub(crate) areas: Vec<NavigationAreaManifest>,
}

#[derive(Debug, Clone, Serialize, Deserialize)]
#[serde(deny_unknown_fields)]
pub(crate) struct NavigationAgentManifest {
    pub(crate) height: f32,
    pub(crate) radius: f32,
    pub(crate) max_climb: f32,
    pub(crate) max_slope_degrees: f32,
}

#[derive(Debug, Clone, Serialize, Deserialize)]
#[serde(deny_unknown_fields)]
pub(crate) struct NavigationBuildManifest {
    pub(crate) cell_size: f32,
    pub(crate) cell_height: f32,
    pub(crate) tile_size: u32,
    pub(crate) region_min_area: u32,
    pub(crate) region_merge_area: u32,
    pub(crate) max_edge_length: f32,
    pub(crate) max_simplification_error: f32,
    pub(crate) max_vertices_per_polygon: u32,
    pub(crate) detail_sample_distance: f32,
    pub(crate) detail_sample_max_error: f32,
}

#[derive(Debug, Clone, Serialize, Deserialize)]
#[serde(deny_unknown_fields)]
pub(crate) struct NavigationAreaManifest {
    pub(crate) key: String,
    pub(crate) traversable: bool,
    #[serde(default)]
    pub(crate) cost: Option<f32>,
}

#[derive(Debug, Clone, Serialize, Deserialize)]
#[serde(deny_unknown_fields)]
pub(crate) struct AudioClipManifest {
    pub(crate) source: PathBuf,
    #[serde(default)]
    pub(crate) loop_region: Option<AudioLoopManifest>,
}

#[derive(Debug, Clone, Serialize, Deserialize)]
#[serde(deny_unknown_fields)]
pub(crate) struct AcousticSceneManifest {
    pub(crate) source: PathBuf,
    pub(crate) materials: Vec<AcousticMaterialManifest>,
}

#[derive(Debug, Clone, Serialize, Deserialize)]
#[serde(deny_unknown_fields)]
pub(crate) struct AudioStreamManifest {
    pub(crate) source: PathBuf,
}

#[derive(Debug, Clone, Serialize, Deserialize)]
#[serde(deny_unknown_fields)]
pub(crate) struct SoundEventManifest {
    pub(crate) media: AssetId,
    pub(crate) gain_db: f32,
    pub(crate) priority: u8,
    pub(crate) spatialization: AudioSpatializationManifest,
    #[serde(default)]
    pub(crate) loop_region: Option<AudioLoopManifest>,
    #[serde(default)]
    pub(crate) attenuation: Option<AudioAttenuationManifest>,
    #[serde(default)]
    pub(crate) concurrency: Option<AudioConcurrencyManifest>,
}

#[derive(Debug, Clone, Copy, Serialize, Deserialize)]
#[serde(deny_unknown_fields)]
pub(crate) struct AudioLoopManifest {
    pub(crate) start_frame: u64,
    pub(crate) end_frame: u64,
}

#[derive(Debug, Clone, Copy, Serialize, Deserialize)]
#[serde(deny_unknown_fields)]
pub(crate) struct AudioAttenuationManifest {
    pub(crate) min_distance: f32,
    pub(crate) max_distance: f32,
}

#[derive(Debug, Clone, Serialize, Deserialize)]
#[serde(deny_unknown_fields)]
pub(crate) struct AudioConcurrencyManifest {
    pub(crate) group: String,
    pub(crate) max_voices: u16,
}

#[derive(Debug, Clone, Serialize, Deserialize)]
#[serde(deny_unknown_fields)]
pub(crate) struct AcousticMaterialManifest {
    pub(crate) id: String,
    pub(crate) absorption: [f32; 3],
    pub(crate) scattering: f32,
    pub(crate) transmission: [f32; 3],
}

#[derive(Debug, Clone, Serialize, Deserialize)]
#[serde(deny_unknown_fields)]
pub(crate) struct AcousticProbesManifest {
    pub(crate) source: PathBuf,
    pub(crate) volume: String,
    pub(crate) scene: AssetId,
    pub(crate) generation: AcousticProbeGenerationManifest,
    pub(crate) spacing_meters: f32,
    pub(crate) height_meters: f32,
}

#[derive(Debug, Clone, Copy, Serialize, Deserialize)]
#[serde(rename_all = "snake_case")]
pub(crate) enum AudioSpatializationManifest {
    TwoDimensional,
    Hrtf,
}

#[derive(Debug, Clone, Copy, Serialize, Deserialize)]
#[serde(rename_all = "snake_case")]
pub(crate) enum AcousticProbeGenerationManifest {
    UniformFloor,
}

#[derive(Debug, Clone, Serialize, Deserialize)]
#[serde(deny_unknown_fields)]
pub(crate) struct AcousticManifest {
    pub(crate) source: PathBuf,
    pub(crate) zones: Vec<AcousticZoneManifest>,
}

#[derive(Debug, Clone, Serialize, Deserialize)]
#[serde(deny_unknown_fields)]
pub(crate) struct AcousticZoneManifest {
    pub(crate) id: String,
    pub(crate) scene: AssetId,
    pub(crate) probes: AssetId,
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
    navigation: Option<NavigationManifest>,
    audio_clip: Option<AudioClipManifest>,
    audio_stream: Option<AudioStreamManifest>,
    sound_event: Option<SoundEventManifest>,
    acoustic_scene: Option<AcousticSceneManifest>,
    acoustic_probes: Option<AcousticProbesManifest>,
    acoustic: Option<AcousticManifest>,
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
    navigation: Option<NavigationManifest>,
    audio_clip: Option<AudioClipManifest>,
    audio_stream: Option<AudioStreamManifest>,
    sound_event: Option<SoundEventManifest>,
    acoustic_scene: Option<AcousticSceneManifest>,
    acoustic_probes: Option<AcousticProbesManifest>,
    acoustic: Option<AcousticManifest>,
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

    #[allow(
        clippy::too_many_lines,
        reason = "the graph validator exhaustively enforces every typed dependency edge"
    )]
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
                validate_dependency_target(id, &asset.manifest.source, &dependency, target)?;
            }
            if let AssetSource::Acoustic(manifest) = &asset.manifest.source {
                self.validate_acoustic_zones(id, manifest)?;
            }
        }
        Ok(())
    }

    fn validate_acoustic_zones(
        &self,
        id: &AssetId,
        manifest: &AcousticManifest,
    ) -> anyhow::Result<()> {
        for zone in &manifest.zones {
            let probes = self.assets.get(&zone.probes).with_context(|| {
                format!(
                    "acoustic environment `{id}` references missing probes `{}`",
                    zone.probes
                )
            })?;
            let AssetSource::AcousticProbes(probes) = &probes.manifest.source else {
                bail!(
                    "acoustic environment `{id}` zone `{}` references a non-probe asset",
                    zone.id
                );
            };
            if probes.scene != zone.scene {
                bail!(
                    "acoustic environment `{id}` zone `{}` pairs probes `{}` with the wrong scene `{}`",
                    zone.id,
                    zone.probes,
                    zone.scene
                );
            }
        }
        Ok(())
    }
}

fn validate_dependency_target(
    id: &AssetId,
    source: &AssetSource,
    dependency: &AssetId,
    target: &LoadedAsset,
) -> anyhow::Result<()> {
    let valid = match source {
        AssetSource::Animation(_) => matches!(target.manifest.source, AssetSource::Skeleton(_)),
        AssetSource::Model(_) => matches!(
            target.manifest.source,
            AssetSource::Mesh(_) | AssetSource::Volume(_)
        ),
        AssetSource::SoundEvent(_) => matches!(
            target.manifest.source,
            AssetSource::AudioClip(_) | AssetSource::AudioStream(_)
        ),
        AssetSource::AcousticProbes(_) => {
            matches!(target.manifest.source, AssetSource::AcousticScene(_))
        }
        AssetSource::Acoustic(_) => matches!(
            target.manifest.source,
            AssetSource::AcousticScene(_) | AssetSource::AcousticProbes(_)
        ),
        AssetSource::Blob(_)
        | AssetSource::Luau(_)
        | AssetSource::Shader(_)
        | AssetSource::Texture(_)
        | AssetSource::Mesh(_)
        | AssetSource::Volume(_)
        | AssetSource::Skeleton(_)
        | AssetSource::Navigation(_)
        | AssetSource::AudioClip(_)
        | AssetSource::AudioStream(_)
        | AssetSource::AcousticScene(_) => true,
    };
    if valid {
        Ok(())
    } else {
        bail!("asset `{id}` dependency `{dependency}` has an incompatible asset kind")
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

#[allow(
    clippy::too_many_lines,
    reason = "loading handles both source-less policies and file-backed asset sections"
)]
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
        navigation: file.navigation,
        audio_clip: file.audio_clip,
        audio_stream: file.audio_stream,
        sound_event: file.sound_event,
        acoustic_scene: file.acoustic_scene,
        acoustic_probes: file.acoustic_probes,
        acoustic: file.acoustic,
    };
    let source = asset_source(file.kind, file.audience, sections, path)?;
    let (source_relative, source_path, source_bytes) = if let Some(source_path) = source.source() {
        let source_relative = portable_relative_path(source_path)
            .with_context(|| format!("invalid source path in `{}`", path.display()))?;
        let source_path = resolve_source(source_root, path, source_path)?;
        let source_bytes = fs::read(&source_path)
            .with_context(|| format!("failed to read `{}`", source_path.display()))?;
        (source_relative, source_path, source_bytes)
    } else {
        (String::new(), path.to_owned(), Vec::new())
    };
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

#[allow(
    clippy::too_many_lines,
    reason = "the exhaustive asset-kind dispatch is the canonical manifest section routing table"
)]
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
        AssetKind::NavigationMesh => {
            let mut navigation = required_section(sections.navigation, "navigation", path)?;
            validate_navigation_manifest(&mut navigation, audience, path)?;
            AssetSource::Navigation(navigation)
        }
        AssetKind::AudioClip => {
            let clip = required_section(sections.audio_clip, "audio_clip", path)?;
            validate_audio_source(&clip.source, audience, "audio_clip", path)?;
            if let Some(region) = clip.loop_region {
                let _region =
                    blackflower_audio_media::LoopRegion::new(region.start_frame, region.end_frame)
                        .with_context(|| {
                            format!("invalid audio clip loop in `{}`", path.display())
                        })?;
            }
            AssetSource::AudioClip(clip)
        }
        AssetKind::AudioStream => {
            let stream = required_section(sections.audio_stream, "audio_stream", path)?;
            validate_audio_source(&stream.source, audience, "audio_stream", path)?;
            AssetSource::AudioStream(stream)
        }
        AssetKind::SoundEvent => {
            let event = required_section(sections.sound_event, "sound_event", path)?;
            validate_sound_event_manifest(&event, audience, path)?;
            AssetSource::SoundEvent(event)
        }
        AssetKind::AcousticScene => {
            let mut acoustic = required_section(sections.acoustic_scene, "acoustic_scene", path)?;
            validate_acoustic_scene_manifest(&mut acoustic, audience, path)?;
            AssetSource::AcousticScene(acoustic)
        }
        AssetKind::AcousticProbeBatch => {
            let probes = required_section(sections.acoustic_probes, "acoustic_probes", path)?;
            validate_acoustic_probes_manifest(&probes, audience, path)?;
            AssetSource::AcousticProbes(probes)
        }
        AssetKind::AcousticEnvironment => {
            let mut acoustic = required_section(sections.acoustic, "acoustic", path)?;
            validate_acoustic_manifest(&mut acoustic, audience, path)?;
            AssetSource::Acoustic(acoustic)
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
            + usize::from(self.navigation.is_some())
            + usize::from(self.audio_clip.is_some())
            + usize::from(self.audio_stream.is_some())
            + usize::from(self.sound_event.is_some())
            + usize::from(self.acoustic_scene.is_some())
            + usize::from(self.acoustic_probes.is_some())
            + usize::from(self.acoustic.is_some())
    }
}

fn validate_audio_source(
    source: &Path,
    audience: AssetAudience,
    kind: &str,
    path: &Path,
) -> anyhow::Result<()> {
    if audience != AssetAudience::Presentation {
        bail!(
            "{kind} asset manifest `{}` must use audience `presentation`",
            path.display()
        );
    }
    let extension = source
        .extension()
        .and_then(|value| value.to_str())
        .map(str::to_ascii_lowercase);
    if !matches!(extension.as_deref(), Some("wav" | "flac")) {
        bail!(
            "{kind} source in `{}` must use `.wav` or `.flac`",
            path.display()
        );
    }
    Ok(())
}

fn validate_acoustic_scene_manifest(
    acoustic: &mut AcousticSceneManifest,
    audience: AssetAudience,
    path: &Path,
) -> anyhow::Result<()> {
    validate_acoustic_audience(audience, path)?;
    validate_gltf_source(&acoustic.source, "acoustic scene", path)?;
    if acoustic.materials.is_empty() {
        bail!(
            "acoustic scene manifest `{}` must declare at least one material",
            path.display()
        );
    }
    acoustic
        .materials
        .sort_by(|left, right| left.id.cmp(&right.id));
    for material in &acoustic.materials {
        blackflower_cooker_acoustics::AcousticMaterialDefinition::new(
            material.id.clone(),
            material.absorption,
            material.scattering,
            material.transmission,
        )
        .with_context(|| {
            format!(
                "invalid acoustic material `{}` in `{}`",
                material.id,
                path.display()
            )
        })?;
    }
    for pair in acoustic.materials.windows(2) {
        if pair[0].id == pair[1].id {
            bail!(
                "duplicate acoustic material `{}` in `{}`",
                pair[0].id,
                path.display()
            );
        }
    }
    Ok(())
}

fn validate_acoustic_probes_manifest(
    probes: &AcousticProbesManifest,
    audience: AssetAudience,
    path: &Path,
) -> anyhow::Result<()> {
    validate_acoustic_audience(audience, path)?;
    validate_gltf_source(&probes.source, "acoustic probes", path)?;
    validate_selection_name(&probes.volume, "acoustic probe volume", path)?;
    if !probes.spacing_meters.is_finite()
        || probes.spacing_meters <= 0.0
        || !probes.height_meters.is_finite()
        || probes.height_meters <= 0.0
    {
        bail!(
            "acoustic probe spacing_meters and height_meters in `{}` must be positive finite values",
            path.display()
        );
    }
    match probes.generation {
        AcousticProbeGenerationManifest::UniformFloor => {}
    }
    Ok(())
}

fn validate_acoustic_manifest(
    acoustic: &mut AcousticManifest,
    audience: AssetAudience,
    path: &Path,
) -> anyhow::Result<()> {
    validate_acoustic_audience(audience, path)?;
    validate_gltf_source(&acoustic.source, "acoustic environment", path)?;
    if acoustic.zones.is_empty() {
        bail!(
            "acoustic environment manifest `{}` must declare at least one zone",
            path.display()
        );
    }
    acoustic.zones.sort_by(|left, right| left.id.cmp(&right.id));
    for zone in &acoustic.zones {
        blackflower_audio_spatial::AcousticZone::new(
            zone.id.clone(),
            zone.scene.as_str(),
            zone.probes.as_str(),
        )
        .with_context(|| {
            format!(
                "invalid acoustic zone `{}` in `{}`",
                zone.id,
                path.display()
            )
        })?;
        if zone.scene == zone.probes {
            bail!(
                "acoustic zone `{}` in `{}` uses the same scene and probe asset",
                zone.id,
                path.display()
            );
        }
    }
    for pair in acoustic.zones.windows(2) {
        if pair[0].id == pair[1].id {
            bail!(
                "duplicate acoustic zone `{}` in `{}`",
                pair[0].id,
                path.display()
            );
        }
    }
    Ok(())
}

fn validate_acoustic_audience(audience: AssetAudience, path: &Path) -> anyhow::Result<()> {
    if audience != AssetAudience::Presentation {
        bail!(
            "acoustic asset manifest `{}` must use audience `presentation`",
            path.display()
        );
    }
    Ok(())
}

fn validate_sound_event_manifest(
    event: &SoundEventManifest,
    audience: AssetAudience,
    path: &Path,
) -> anyhow::Result<()> {
    if audience != AssetAudience::Presentation {
        bail!(
            "sound_event asset manifest `{}` must use audience `presentation`",
            path.display()
        );
    }
    let loop_region = event
        .loop_region
        .map(|region| {
            blackflower_audio_media::LoopRegion::new(region.start_frame, region.end_frame)
        })
        .transpose()?;
    let attenuation = event
        .attenuation
        .map(|value| blackflower_audio_media::Attenuation {
            min_distance: value.min_distance,
            max_distance: value.max_distance,
        });
    let concurrency =
        event
            .concurrency
            .as_ref()
            .map(|value| blackflower_audio_media::Concurrency {
                group: value.group.clone(),
                max_voices: value.max_voices,
            });
    blackflower_audio_media::SoundEvent {
        media: event.media.clone(),
        gain_db: event.gain_db,
        priority: event.priority,
        spatialization: match event.spatialization {
            AudioSpatializationManifest::TwoDimensional => {
                blackflower_audio_media::Spatialization::TwoDimensional
            }
            AudioSpatializationManifest::Hrtf => blackflower_audio_media::Spatialization::Hrtf,
        },
        loop_region,
        attenuation,
        concurrency,
    }
    .validate()
    .with_context(|| format!("invalid sound event policy in `{}`", path.display()))
}

fn required_section<T>(section: Option<T>, name: &str, path: &Path) -> anyhow::Result<T> {
    section.with_context(|| {
        format!(
            "{name} asset manifest `{}` requires a `[{name}]` section",
            path.display()
        )
    })
}

#[allow(
    clippy::too_many_lines,
    reason = "navigation validation checks one deliberately complete inheritance-free manifest contract"
)]
fn validate_navigation_manifest(
    navigation: &mut NavigationManifest,
    audience: AssetAudience,
    path: &Path,
) -> anyhow::Result<()> {
    if audience != AssetAudience::Simulation {
        bail!(
            "navigation asset manifest `{}` must use audience `simulation`",
            path.display()
        );
    }
    validate_gltf_source(&navigation.source, "navigation", path)?;
    blackflower_navigation::NavAgentProfileId::new(navigation.profile_id.clone())
        .with_context(|| format!("invalid navigation profile_id in `{}`", path.display()))?;
    blackflower_navigation::NavAgentProfile::new(
        blackflower_navigation::NavAgentProfileId::new(navigation.profile_id.clone())?,
        navigation.agent.height,
        navigation.agent.radius,
        navigation.agent.max_climb,
        navigation.agent.max_slope_degrees,
    )
    .with_context(|| format!("invalid navigation agent in `{}`", path.display()))?;
    blackflower_navigation::NavigationBuildSettings::new(
        navigation.build.cell_size,
        navigation.build.cell_height,
        navigation.build.tile_size,
        navigation.build.region_min_area,
        navigation.build.region_merge_area,
        navigation.build.max_edge_length,
        navigation.build.max_simplification_error,
        navigation.build.max_vertices_per_polygon,
        navigation.build.detail_sample_distance,
        navigation.build.detail_sample_max_error,
    )
    .with_context(|| format!("invalid navigation build settings in `{}`", path.display()))?;
    if navigation.areas.is_empty() || navigation.areas.len() > blackflower_navigation::MAX_AREAS {
        bail!(
            "navigation manifest `{}` must declare from 1 through 64 areas",
            path.display()
        );
    }
    navigation
        .areas
        .sort_by(|left, right| left.key.cmp(&right.key));
    for (index, area) in navigation.areas.iter().enumerate() {
        let key = blackflower_navigation::NavigationAreaKey::new(area.key.clone()).with_context(
            || {
                format!(
                    "invalid navigation area key `{}` in `{}`",
                    area.key,
                    path.display()
                )
            },
        )?;
        let id = u8::try_from(index).context("navigation area index exceeds u8")?;
        blackflower_navigation::NavigationArea::new(id, key, area.traversable, area.cost)
            .with_context(|| {
                format!(
                    "invalid navigation area `{}` in `{}`",
                    area.key,
                    path.display()
                )
            })?;
    }
    for pair in navigation.areas.windows(2) {
        if pair[0].key == pair[1].key {
            bail!(
                "duplicate navigation area `{}` in `{}`",
                pair[0].key,
                path.display()
            );
        }
    }
    if !navigation.areas.iter().any(|area| area.traversable) {
        bail!(
            "navigation manifest `{}` must declare at least one traversable area",
            path.display()
        );
    }
    Ok(())
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

#[cfg(test)]
mod tests {
    use std::fs;
    use std::str::FromStr;

    use anyhow::Context;
    use blackflower_assets::{AssetAudience, AssetId, AssetKind, PackageName};
    use tempfile::TempDir;

    use super::{AssetSource, Repository};

    #[test]
    #[allow(
        clippy::too_many_lines,
        reason = "the manifest fixture spells out every required navigation field"
    )]
    fn navigation_manifest_is_explicit_and_canonicalizes_areas() -> anyhow::Result<()> {
        let directory = TempDir::new()?;
        let asset = directory.path().join("level");
        fs::create_dir(&asset)?;
        fs::write(
            asset.join("navigation.gltf"),
            br#"{"asset":{"version":"2.0"}}"#,
        )?;
        fs::write(
            asset.join("asset.toml"),
            r#"
schema = 1
id = "levels/test/navigation/humanoid"
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
region_min_area = 8
region_merge_area = 20
max_edge_length = 12.0
max_simplification_error = 1.3
max_vertices_per_polygon = 6
detail_sample_distance = 6.0
detail_sample_max_error = 1.0

[[navigation.areas]]
key = "water"
traversable = false

[[navigation.areas]]
key = "ground"
traversable = true
cost = 1.0
"#,
        )?;

        let repository = Repository::load(directory.path())?;
        let loaded = repository
            .assets
            .values()
            .next()
            .context("navigation asset was not loaded")?;
        assert_eq!(loaded.manifest.kind(), AssetKind::NavigationMesh);
        assert_eq!(loaded.manifest.audience, AssetAudience::Simulation);
        let AssetSource::Navigation(navigation) = &loaded.manifest.source else {
            anyhow::bail!("loaded asset is not navigation");
        };
        assert_eq!(navigation.areas[0].key, "ground");
        assert_eq!(navigation.areas[1].key, "water");
        Ok(())
    }

    #[test]
    #[allow(
        clippy::too_many_lines,
        reason = "the fixture spells out both sides of the source-less audio dependency"
    )]
    fn sound_event_is_source_less_and_closes_over_media() -> anyhow::Result<()> {
        let directory = TempDir::new()?;
        let media = directory.path().join("media");
        let event = directory.path().join("event");
        let package = directory.path().join("packages/pak000");
        fs::create_dir(&media)?;
        fs::create_dir(&event)?;
        fs::create_dir_all(&package)?;
        fs::write(media.join("shot.wav"), b"fixture")?;
        fs::write(
            media.join("asset.toml"),
            r#"
schema = 1
id = "audio/shot"
kind = "audio_clip"
audience = "presentation"

[audio_clip]
source = "shot.wav"
"#,
        )?;
        fs::write(
            event.join("asset.toml"),
            r#"
schema = 1
id = "sound_events/shot"
kind = "sound_event"
audience = "presentation"

[sound_event]
media = "audio/shot"
gain_db = 0.0
priority = 100
spatialization = "hrtf"
"#,
        )?;
        fs::write(
            package.join("package.toml"),
            r#"
schema = 1
assets = ["sound_events/shot"]
"#,
        )?;

        let repository = Repository::load(directory.path())?;
        let selected = repository.selected_assets(&PackageName::from_str("pak000")?)?;
        assert_eq!(selected.len(), 2);
        assert!(selected.contains(&AssetId::from_str("audio/shot")?));
        let event_id = AssetId::from_str("sound_events/shot")?;
        let loaded = repository
            .assets
            .get(&event_id)
            .context("sound event was not loaded")?;
        assert!(loaded.source_bytes.is_empty());
        assert_eq!(loaded.manifest.kind(), AssetKind::SoundEvent);
        Ok(())
    }
}
