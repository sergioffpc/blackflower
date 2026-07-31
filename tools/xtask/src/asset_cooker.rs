use std::collections::{BTreeMap, BTreeSet};

use anyhow::{Context, bail};
use blackflower_assets::{AssetAudience, AssetId, AssetKind, Bytes, ContentHash, RecipeHash};
use blackflower_audio_media::{
    Attenuation, Concurrency, LoopRegion, SoundEvent, Spatialization, cook_clip, cook_stream,
};
use blackflower_scripting::{compile, luau_version};
use blackflower_shader_compiler::{compile as compile_shader, slang_version};
use naga::front::spv;
use naga::valid::{Capabilities, ValidationFlags, Validator};

use crate::acoustic_cooker;
use crate::manifest::{
    AssetSource, AudioSpatializationManifest, LoadedAsset, Repository, SoundEventManifest,
};
use crate::mesh_cooker;
use crate::model_cooker;
use crate::navigation_cooker;
use crate::profile::CookingProfile;
use crate::texture_cooker;

pub(crate) const HALF_VERSION: &str = "2.7.1";
pub(crate) const IMAGE_VERSION: &str = "0.25.10";
pub(crate) const NAGA_VERSION: &str = "30.0.0";

#[derive(Debug)]
pub(crate) struct CookedAsset {
    pub(crate) kind: AssetKind,
    pub(crate) audience: AssetAudience,
    pub(crate) dependencies: Vec<AssetId>,
    pub(crate) bytes: Bytes,
    pub(crate) content_hash: ContentHash,
    pub(crate) recipe_hash: RecipeHash,
}

struct CookedPayload {
    bytes: Bytes,
    derived_source_hash: Option<blake3::Hash>,
}

impl CookedPayload {
    const fn plain(bytes: Bytes) -> Self {
        Self {
            bytes,
            derived_source_hash: None,
        }
    }
}

pub(crate) fn cook_assets(
    repository: &Repository,
    selected: &BTreeSet<AssetId>,
    profile: &CookingProfile,
) -> anyhow::Result<BTreeMap<AssetId, CookedAsset>> {
    let mut cooked = BTreeMap::new();
    let mut pending = selected.clone();
    while !pending.is_empty() {
        let Some(id) = pending
            .iter()
            .find(|id| {
                repository.assets.get(*id).is_some_and(|asset| {
                    asset
                        .manifest
                        .dependencies()
                        .iter()
                        .all(|dependency| cooked.contains_key(dependency))
                })
            })
            .cloned()
        else {
            bail!("selected asset dependency graph cannot be cooked");
        };
        let source = repository
            .assets
            .get(&id)
            .with_context(|| format!("missing selected asset `{id}`"))?;
        let payload = cook_asset(repository, source, profile, &cooked)
            .with_context(|| format!("failed to cook asset `{id}`"))?;
        let dependencies = source.manifest.dependencies();
        let content_hash = ContentHash::hash_bytes(&payload.bytes);
        let recipe_hash = recipe_hash(
            source,
            profile,
            payload.derived_source_hash.as_ref(),
            &dependencies,
            &cooked,
        )?;
        cooked.insert(
            id.clone(),
            CookedAsset {
                kind: source.manifest.kind(),
                audience: source.manifest.audience,
                dependencies,
                bytes: payload.bytes,
                content_hash,
                recipe_hash,
            },
        );
        pending.remove(&id);
    }
    Ok(cooked)
}

#[allow(
    clippy::too_many_lines,
    reason = "the exhaustive source-kind dispatch is the canonical cooker routing table"
)]
fn cook_asset(
    repository: &Repository,
    source: &LoadedAsset,
    profile: &CookingProfile,
    cooked: &BTreeMap<AssetId, CookedAsset>,
) -> anyhow::Result<CookedPayload> {
    match &source.manifest.source {
        AssetSource::Blob(_) => Ok(CookedPayload::plain(Bytes::from(
            source.source_bytes.clone(),
        ))),
        AssetSource::Luau(_) => {
            let text =
                std::str::from_utf8(&source.source_bytes).context("Luau source is not UTF-8")?;
            let bytecode = compile(text, profile.scripting.luau.compile_options())
                .context("Luau compiler rejected source")?;
            Ok(CookedPayload::plain(Bytes::from(bytecode.into_bytes())))
        }
        AssetSource::Shader(manifest) => {
            let text =
                std::str::from_utf8(&source.source_bytes).context("Slang source is not UTF-8")?;
            let options = profile.shaders.compile_options(manifest.stage.into());
            let source_name = format!("{}/{}", source.manifest.id.as_str(), source.source_relative);
            let spirv = compile_shader(&source_name, text, &manifest.entry_point, options)
                .context("Slang compiler rejected source")?;
            validate_spirv(&spirv)?;
            Ok(CookedPayload::plain(spirv))
        }
        AssetSource::Texture(manifest) => {
            texture_cooker::cook(source, manifest, profile.textures).map(CookedPayload::plain)
        }
        AssetSource::Mesh(manifest) => {
            let mesh = mesh_cooker::cook(source, manifest, &profile.meshes)?;
            Ok(CookedPayload {
                bytes: mesh.bytes,
                derived_source_hash: Some(mesh.source_hash),
            })
        }
        AssetSource::Model(manifest) => {
            model_cooker::cook(source, manifest, repository).map(CookedPayload::plain)
        }
        AssetSource::Volume(manifest) => {
            blackflower_cooker_volume::cook(&source.source_path, &manifest.grids)
                .context("volume cooker rejected OpenVDB source")
                .map(CookedPayload::plain)
        }
        AssetSource::Skeleton(manifest) => cook_skeleton(source, &manifest.skin),
        AssetSource::Animation(manifest) => {
            cook_animation(source, &manifest.clip, &manifest.skeleton, profile, cooked)
        }
        AssetSource::Navigation(manifest) => {
            let navigation = navigation_cooker::cook(source, manifest)?;
            Ok(CookedPayload {
                bytes: navigation.bytes,
                derived_source_hash: Some(navigation.source_hash),
            })
        }
        AssetSource::AudioClip(manifest) => {
            let extension = source_extension(source)?;
            let loop_region = manifest
                .loop_region
                .map(|region| LoopRegion::new(region.start_frame, region.end_frame))
                .transpose()?;
            cook_clip(extension, &source.source_bytes, loop_region)
                .context("audio clip cooker rejected source")
                .map(|bytes| CookedPayload::plain(Bytes::from(bytes)))
        }
        AssetSource::AudioStream(_) => {
            let extension = source_extension(source)?;
            cook_stream(extension, &source.source_bytes, profile.audio)
                .context("audio stream cooker rejected source")
                .map(|bytes| CookedPayload::plain(Bytes::from(bytes)))
        }
        AssetSource::SoundEvent(manifest) => sound_event(manifest)?
            .to_bytes()
            .context("sound event cooker rejected policy")
            .map(|bytes| CookedPayload::plain(Bytes::from(bytes))),
        AssetSource::AcousticScene(manifest) => {
            let acoustic = acoustic_cooker::cook_scene(source, manifest, cooked)?;
            Ok(CookedPayload {
                bytes: acoustic.bytes,
                derived_source_hash: acoustic.source_hash,
            })
        }
        AssetSource::AcousticProbes(manifest) => {
            let acoustic =
                acoustic_cooker::cook_probes(source, manifest, profile.acoustics, cooked)?;
            Ok(CookedPayload {
                bytes: acoustic.bytes,
                derived_source_hash: acoustic.source_hash,
            })
        }
        AssetSource::Acoustic(manifest) => {
            let acoustic = acoustic_cooker::cook_environment(source, manifest, cooked)?;
            Ok(CookedPayload {
                bytes: acoustic.bytes,
                derived_source_hash: acoustic.source_hash,
            })
        }
        AssetSource::AcousticMaterials(manifest) => {
            let acoustic = acoustic_cooker::cook_materials(manifest)?;
            Ok(CookedPayload {
                bytes: acoustic.bytes,
                derived_source_hash: acoustic.source_hash,
            })
        }
        AssetSource::AcousticTopology(manifest) => {
            let acoustic = acoustic_cooker::cook_topology(source, manifest)?;
            Ok(CookedPayload {
                bytes: acoustic.bytes,
                derived_source_hash: acoustic.source_hash,
            })
        }
        AssetSource::AcousticPrefab(manifest) => {
            let acoustic = acoustic_cooker::cook_prefab(source, manifest, cooked)?;
            Ok(CookedPayload {
                bytes: acoustic.bytes,
                derived_source_hash: acoustic.source_hash,
            })
        }
        AssetSource::AcousticSimulation(manifest) => {
            let acoustic = acoustic_cooker::cook_simulation(source, manifest, cooked)?;
            Ok(CookedPayload {
                bytes: acoustic.bytes,
                derived_source_hash: acoustic.source_hash,
            })
        }
        AssetSource::AcousticEmission(manifest) => {
            let acoustic = acoustic_cooker::cook_emission(manifest, repository, profile)?;
            Ok(CookedPayload {
                bytes: acoustic.bytes,
                derived_source_hash: acoustic.source_hash,
            })
        }
    }
}

fn source_extension(source: &LoadedAsset) -> anyhow::Result<&str> {
    source
        .source_path
        .extension()
        .and_then(|value| value.to_str())
        .context("audio source extension is not UTF-8")
}

fn sound_event(manifest: &SoundEventManifest) -> anyhow::Result<SoundEvent> {
    Ok(SoundEvent {
        media: manifest.media.clone(),
        gain_db: manifest.gain_db,
        priority: manifest.priority,
        spatialization: match manifest.spatialization {
            AudioSpatializationManifest::TwoDimensional => Spatialization::TwoDimensional,
            AudioSpatializationManifest::Hrtf => Spatialization::Hrtf,
        },
        loop_region: manifest
            .loop_region
            .map(|region| LoopRegion::new(region.start_frame, region.end_frame))
            .transpose()?,
        attenuation: manifest.attenuation.map(|value| Attenuation {
            min_distance: value.min_distance,
            max_distance: value.max_distance,
        }),
        concurrency: manifest.concurrency.as_ref().map(|value| Concurrency {
            group: value.group.clone(),
            max_voices: value.max_voices,
        }),
    })
}

fn cook_skeleton(source: &LoadedAsset, skin: &str) -> anyhow::Result<CookedPayload> {
    let bytes = blackflower_cooker_animation::cook_skeleton(&source.source_path, skin)
        .context("animation cooker rejected skeleton source")?;
    Ok(CookedPayload {
        derived_source_hash: Some(blake3::hash(&bytes)),
        bytes: Bytes::from(bytes),
    })
}

fn cook_animation(
    source: &LoadedAsset,
    clip: &str,
    skeleton_id: &AssetId,
    profile: &CookingProfile,
    cooked: &BTreeMap<AssetId, CookedAsset>,
) -> anyhow::Result<CookedPayload> {
    let skeleton = cooked
        .get(skeleton_id)
        .with_context(|| format!("skeleton dependency `{skeleton_id}` was not cooked"))?;
    let bytes = blackflower_cooker_animation::cook_animation(
        &source.source_path,
        clip,
        &skeleton.bytes,
        profile.animations.into(),
    )
    .context("animation cooker rejected clip source")?;
    Ok(CookedPayload {
        derived_source_hash: Some(blake3::hash(&bytes)),
        bytes: Bytes::from(bytes),
    })
}

fn validate_spirv(bytes: &[u8]) -> anyhow::Result<()> {
    let options = spv::Options {
        adjust_coordinate_space: false,
        strict_capabilities: true,
        block_ctx_dump_prefix: None,
    };
    let module =
        spv::parse_u8_slice(bytes, &options).context("Naga rejected generated SPIR-V syntax")?;
    Validator::new(ValidationFlags::all(), Capabilities::empty())
        .validate(&module)
        .context("Naga rejected generated SPIR-V semantics")?;
    Ok(())
}

#[allow(
    clippy::too_many_lines,
    reason = "the exhaustive source-kind match is the single canonical recipe definition"
)]
fn recipe_hash(
    source: &LoadedAsset,
    profile: &CookingProfile,
    derived_source_hash: Option<&blake3::Hash>,
    dependencies: &[AssetId],
    cooked: &BTreeMap<AssetId, CookedAsset>,
) -> anyhow::Result<RecipeHash> {
    let mut hasher = CanonicalHasher::new(b"blackflower.asset-recipe.v2");
    hasher.u32(source.manifest.schema);
    hasher.text(source.manifest.id.as_str());
    hasher.serializable(&source.manifest.kind())?;
    hasher.serializable(&source.manifest.audience)?;
    hasher.text(&source.source_relative);
    if !matches!(
        &source.manifest.source,
        AssetSource::AcousticScene(_)
            | AssetSource::AcousticProbes(_)
            | AssetSource::Acoustic(_)
            | AssetSource::AcousticTopology(_)
            | AssetSource::AcousticPrefab(_)
            | AssetSource::AcousticSimulation(_)
            | AssetSource::AcousticEmission(_)
    ) {
        hasher.bytes(source.source_hash.as_bytes());
    }
    for dependency in dependencies {
        let dependency_asset = cooked
            .get(dependency)
            .with_context(|| format!("missing cooked dependency `{dependency}`"))?;
        hasher.text(dependency.as_str());
        hasher.bytes(dependency_asset.content_hash.as_bytes());
    }
    hasher.text(env!("CARGO_PKG_VERSION"));
    match &source.manifest.source {
        AssetSource::Blob(_) => hasher.text("blob"),
        AssetSource::Luau(_) => {
            hasher.text("luau");
            hasher.serializable(&profile.scripting.luau)?;
            let (major, minor, patch) = luau_version();
            hasher.u32(major);
            hasher.u32(minor);
            hasher.u32(patch);
        }
        AssetSource::Shader(manifest) => {
            hasher.text("shader");
            hasher.text(&manifest.entry_point);
            hasher.serializable(&manifest.stage)?;
            hasher.serializable(&profile.shaders)?;
            hasher.text(slang_version());
            hasher.text(NAGA_VERSION);
        }
        AssetSource::Texture(manifest) => {
            hasher.text("texture");
            hasher.serializable(&manifest.semantic)?;
            hasher.serializable(&profile.textures)?;
            hasher.text(blackflower_rendering_textures::ktx_version());
            hasher.text(IMAGE_VERSION);
            hasher.text(HALF_VERSION);
            hasher.text(&texture_encoder_platform());
        }
        AssetSource::Mesh(manifest) => {
            hasher.text("mesh");
            hasher.text(&manifest.mesh);
            hasher.serializable(&profile.meshes)?;
            hasher.text(mesh_cooker::MESHOPT_VERSION);
            let source_hash =
                derived_source_hash.context("cooked mesh is missing its buffer source hash")?;
            hasher.bytes(source_hash.as_bytes());
        }
        AssetSource::Model(manifest) => {
            hasher.text("model");
            hasher.text(&manifest.scene);
            for attachment in &manifest.attachments {
                hasher.text(&attachment.node);
                hasher.text(attachment.asset.as_str());
            }
            hasher.text(model_cooker::COOKER_RECIPE);
        }
        AssetSource::Volume(manifest) => {
            hasher.text("volume");
            for grid in &manifest.grids {
                hasher.text(grid);
            }
            hasher.text(blackflower_cooker_volume::COOKER_RECIPE);
            hasher.text(blackflower_cooker_volume::OPENVDB_REVISION);
        }
        AssetSource::Skeleton(manifest) => {
            hasher.text("skeleton");
            hasher.text(&manifest.skin);
            hasher.serializable(&profile.animations)?;
            hasher.text(blackflower_cooker_animation::COOKER_RECIPE);
            hasher.text(blackflower_cooker_animation::OZZ_VERSION);
            hasher.text(blackflower_cooker_animation::OZZ_REVISION);
            hasher.u32(u32::from(blackflower_animation_format::CONTAINER_SCHEMA));
            let source_hash = derived_source_hash
                .context("cooked skeleton is missing its derived source hash")?;
            hasher.bytes(source_hash.as_bytes());
        }
        AssetSource::Animation(manifest) => {
            hasher.text("animation_clip");
            hasher.text(&manifest.clip);
            hasher.text(manifest.skeleton.as_str());
            hasher.serializable(&profile.animations)?;
            hasher.text(blackflower_cooker_animation::COOKER_RECIPE);
            hasher.text(blackflower_cooker_animation::OZZ_VERSION);
            hasher.text(blackflower_cooker_animation::OZZ_REVISION);
            hasher.u32(u32::from(blackflower_animation_format::CONTAINER_SCHEMA));
            let source_hash = derived_source_hash
                .context("cooked animation is missing its derived source hash")?;
            hasher.bytes(source_hash.as_bytes());
        }
        AssetSource::Navigation(manifest) => {
            hasher.text("navigation_mesh");
            hasher.serializable(manifest)?;
            hasher.text(blackflower_cooker_navigation::COOKER_RECIPE);
            hasher.text(blackflower_cooker_navigation::RECAST_VERSION);
            hasher.text(blackflower_cooker_navigation::RECAST_REVISION);
            hasher.u32(blackflower_navigation::NAVIGATION_ASSET_SCHEMA);
            hasher.text(&navigation_cooker::platform_identity());
            let source_hash = derived_source_hash
                .context("cooked navigation is missing its buffer source hash")?;
            hasher.bytes(source_hash.as_bytes());
        }
        AssetSource::AudioClip(manifest) => {
            hasher.text("audio_clip");
            hasher.serializable(manifest)?;
            hasher.text(blackflower_audio_media::COOKER_RECIPE);
            hasher.u32(blackflower_audio_media::AUDIO_CLIP_SCHEMA);
        }
        AssetSource::AudioStream(manifest) => {
            hasher.text("audio_stream");
            hasher.serializable(manifest)?;
            hasher.serializable(&profile.audio)?;
            hasher.text(blackflower_audio_media::COOKER_RECIPE);
            hasher.text(blackflower_audio_media::CLAXON_VERSION);
        }
        AssetSource::SoundEvent(manifest) => {
            hasher.text("sound_event");
            hasher.serializable(manifest)?;
            hasher.text(blackflower_audio_media::COOKER_RECIPE);
            hasher.u32(blackflower_audio_media::SOUND_EVENT_SCHEMA);
        }
        AssetSource::AcousticScene(manifest) => {
            hasher.text("acoustic_scene");
            hasher.serializable(manifest)?;
            hasher.text(blackflower_cooker_acoustics::COOKER_RECIPE);
            hasher.text(blackflower_cooker_acoustics::STEAM_AUDIO_REVISION);
            hasher.u32(blackflower_audio_spatial::ACOUSTIC_ASSET_SCHEMA);
            hasher.text(&acoustic_cooker::platform_identity());
            let source_hash = derived_source_hash
                .context("cooked acoustic scene is missing its buffer source hash")?;
            hasher.bytes(source_hash.as_bytes());
        }
        AssetSource::AcousticProbes(manifest) => {
            hasher.text("acoustic_probe_batch");
            hasher.serializable(manifest)?;
            hasher.serializable(&profile.acoustics)?;
            hasher.text(blackflower_cooker_acoustics::COOKER_RECIPE);
            hasher.text(blackflower_cooker_acoustics::STEAM_AUDIO_REVISION);
            hasher.u32(blackflower_audio_spatial::ACOUSTIC_ASSET_SCHEMA);
            hasher.text(&acoustic_cooker::platform_identity());
            let source_hash = derived_source_hash
                .context("cooked acoustic probes are missing their buffer source hash")?;
            hasher.bytes(source_hash.as_bytes());
        }
        AssetSource::Acoustic(manifest) => {
            hasher.text("acoustic_environment");
            hasher.serializable(manifest)?;
            hasher.u32(blackflower_audio_spatial::ACOUSTIC_ENVIRONMENT_SCHEMA);
        }
        AssetSource::AcousticMaterials(manifest) => {
            hasher.text("acoustic_material_library");
            hasher.serializable(manifest)?;
            hasher.text(blackflower_cooker_acoustics::AUTHORITATIVE_COOKER_RECIPE);
            hasher.u32(blackflower_acoustics::ACOUSTIC_ASSET_SCHEMA);
        }
        AssetSource::AcousticTopology(manifest) => {
            hasher.text("acoustic_topology");
            hasher.serializable(manifest)?;
            hasher.text(blackflower_cooker_acoustics::AUTHORITATIVE_COOKER_RECIPE);
            hasher.u32(blackflower_acoustics::ACOUSTIC_ASSET_SCHEMA);
            let source_hash = derived_source_hash
                .context("cooked acoustic topology is missing its derived source hash")?;
            hasher.bytes(source_hash.as_bytes());
        }
        AssetSource::AcousticPrefab(manifest) => {
            hasher.text("acoustic_prefab");
            hasher.serializable(manifest)?;
            hasher.text(blackflower_cooker_acoustics::AUTHORITATIVE_COOKER_RECIPE);
            hasher.u32(blackflower_acoustics::ACOUSTIC_ASSET_SCHEMA);
            let source_hash = derived_source_hash
                .context("cooked acoustic prefab is missing its derived source hash")?;
            hasher.bytes(source_hash.as_bytes());
        }
        AssetSource::AcousticSimulation(manifest) => {
            hasher.text("acoustic_simulation_scene");
            hasher.serializable(manifest)?;
            hasher.text(blackflower_cooker_acoustics::AUTHORITATIVE_COOKER_RECIPE);
            hasher.u32(blackflower_acoustics::ACOUSTIC_ASSET_SCHEMA);
            let source_hash = derived_source_hash
                .context("cooked acoustic simulation scene is missing its derived source hash")?;
            hasher.bytes(source_hash.as_bytes());
        }
        AssetSource::AcousticEmission(manifest) => {
            hasher.text("acoustic_emission_profile");
            hasher.serializable(manifest)?;
            hasher.serializable(&profile.audio)?;
            hasher.text(blackflower_cooker_acoustics::AUTHORITATIVE_COOKER_RECIPE);
            hasher.text(blackflower_audio_media::COOKER_RECIPE);
            hasher.u32(blackflower_acoustics::ACOUSTIC_ASSET_SCHEMA);
            let source_hash = derived_source_hash
                .context("cooked acoustic emission profile is missing its media hash")?;
            hasher.bytes(source_hash.as_bytes());
        }
    }
    Ok(RecipeHash::from_bytes(*hasher.finish().as_bytes()))
}

pub(crate) fn texture_encoder_platform() -> String {
    format!("{}/{}", std::env::consts::OS, std::env::consts::ARCH)
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
