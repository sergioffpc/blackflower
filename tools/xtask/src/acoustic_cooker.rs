use std::collections::BTreeMap;

use anyhow::Context;
use blackflower_assets::{AssetId, Bytes};
use blackflower_audio_spatial::{AcousticEnvironment, AcousticScene, AcousticZone, ProbeBatch};

use crate::asset_cooker::CookedAsset;
use crate::manifest::{
    AcousticEmissionProfileManifest, AcousticManifest, AcousticMaterialLibraryManifest,
    AcousticPrefabManifest, AcousticProbesManifest, AcousticSceneManifest,
    AcousticSimulationManifest, AcousticSoundClassManifest, AcousticTopologyManifest, AssetSource,
    LoadedAsset, Repository,
};
use crate::profile::{AcousticsProfile, CookingProfile};

pub(crate) struct CookedAcoustic {
    pub(crate) bytes: Bytes,
    pub(crate) source_hash: Option<blake3::Hash>,
}

pub(crate) fn cook_scene(
    source: &LoadedAsset,
    manifest: &AcousticSceneManifest,
    cooked: &BTreeMap<AssetId, CookedAsset>,
) -> anyhow::Result<CookedAcoustic> {
    let materials = material_definitions(&manifest.materials, cooked)?;
    let cooked = blackflower_cooker_acoustics::cook_scene(&source.source_path, &materials)
        .context("static acoustic scene cooker rejected glTF source")?;
    Ok(CookedAcoustic {
        bytes: Bytes::copy_from_slice(cooked.asset.bytes()),
        source_hash: Some(cooked.source_hash),
    })
}

pub(crate) fn cook_probes(
    source: &LoadedAsset,
    manifest: &AcousticProbesManifest,
    profile: AcousticsProfile,
    cooked: &BTreeMap<AssetId, CookedAsset>,
) -> anyhow::Result<CookedAcoustic> {
    let scene = cooked.get(&manifest.scene).with_context(|| {
        format!(
            "acoustic scene dependency `{}` was not cooked",
            manifest.scene
        )
    })?;
    let scene = AcousticScene::from_bytes(&scene.bytes)
        .context("acoustic scene dependency is not a valid .bfacscn")?;
    let cooked = blackflower_cooker_acoustics::cook_probe_batch(
        &source.source_path,
        &scene,
        &manifest.volume,
        manifest.spacing_meters,
        manifest.height_meters,
        profile.bake_profile()?,
    )
    .context("acoustic probe cooker rejected glTF source")?;
    Ok(CookedAcoustic {
        bytes: Bytes::copy_from_slice(cooked.asset.bytes()),
        source_hash: Some(cooked.source_hash),
    })
}

pub(crate) fn cook_environment(
    source: &LoadedAsset,
    manifest: &AcousticManifest,
    cooked: &BTreeMap<AssetId, CookedAsset>,
) -> anyhow::Result<CookedAcoustic> {
    let zone_ids = manifest
        .zones
        .iter()
        .map(|zone| zone.id.clone())
        .collect::<Vec<_>>();
    blackflower_cooker_acoustics::validate_environment_zones(&source.source_path, &zone_ids)
        .context("acoustic environment references invalid glTF zones")?;
    let zones = manifest
        .zones
        .iter()
        .map(|zone| {
            let scene = cooked
                .get(&zone.scene)
                .with_context(|| format!("acoustic scene `{}` was not cooked", zone.scene))?;
            let _scene = AcousticScene::from_bytes(&scene.bytes)
                .with_context(|| format!("asset `{}` is not a valid .bfacscn", zone.scene))?;
            let probes = cooked
                .get(&zone.probes)
                .with_context(|| format!("acoustic probes `{}` were not cooked", zone.probes))?;
            let probes = ProbeBatch::from_bytes(&probes.bytes)
                .with_context(|| format!("asset `{}` is not a valid .bfacprb", zone.probes))?;
            if probes.zone() != zone.id {
                anyhow::bail!(
                    "probe batch `{}` belongs to zone `{}`, not `{}`",
                    zone.probes,
                    probes.zone(),
                    zone.id
                );
            }
            AcousticZone::new(zone.id.clone(), zone.scene.as_str(), zone.probes.as_str())
                .map_err(anyhow::Error::from)
        })
        .collect::<anyhow::Result<Vec<_>>>()?;
    let topology = cooked
        .get(&manifest.topology)
        .with_context(|| format!("acoustic topology `{}` was not cooked", manifest.topology))?;
    let _topology = blackflower_acoustics::AcousticTopology::from_bytes(&topology.bytes)
        .with_context(|| format!("asset `{}` is not a valid .bfactpl", manifest.topology))?;
    let environment = AcousticEnvironment::new(manifest.topology.as_str(), zones)?;
    Ok(CookedAcoustic {
        bytes: Bytes::copy_from_slice(environment.bytes()),
        source_hash: None,
    })
}

pub(crate) fn cook_materials(
    manifest: &AcousticMaterialLibraryManifest,
) -> anyhow::Result<CookedAcoustic> {
    let definitions = manifest
        .materials
        .iter()
        .map(|material| {
            blackflower_cooker_acoustics::AcousticMaterialDefinition::new(
                material.id.clone(),
                material.absorption,
                material.scattering,
                material.transmission,
            )
        })
        .collect::<Result<Vec<_>, _>>()?;
    let library = blackflower_cooker_acoustics::cook_material_library(&definitions)?;
    let bytes = library.to_bytes()?;
    Ok(CookedAcoustic {
        source_hash: Some(blake3::hash(&bytes)),
        bytes: Bytes::from(bytes),
    })
}

pub(crate) fn cook_topology(
    source: &LoadedAsset,
    manifest: &AcousticTopologyManifest,
) -> anyhow::Result<CookedAcoustic> {
    let instances = manifest
        .instances
        .iter()
        .map(
            |instance| blackflower_cooker_acoustics::AcousticPrefabInstanceDefinition {
                id: instance.id,
                prefab: instance.prefab.as_str().to_owned(),
                default_state: instance.default_state,
                zones: instance.zones.clone(),
            },
        )
        .collect::<Vec<_>>();
    let topology = blackflower_cooker_acoustics::cook_topology(&source.source_path, &instances)?;
    let bytes = topology.to_bytes()?;
    Ok(CookedAcoustic {
        source_hash: Some(blake3::hash(&bytes)),
        bytes: Bytes::from(bytes),
    })
}

pub(crate) fn cook_prefab(
    source: &LoadedAsset,
    manifest: &AcousticPrefabManifest,
    cooked: &BTreeMap<AssetId, CookedAsset>,
) -> anyhow::Result<CookedAcoustic> {
    let materials = material_definitions(&manifest.materials, cooked)?;
    let states = manifest
        .states
        .iter()
        .map(
            |state| blackflower_cooker_acoustics::AcousticPrefabStateDefinition {
                id: state.id,
                name: state.name.clone(),
                node: state.node.clone(),
            },
        )
        .collect::<Vec<_>>();
    let prefab = blackflower_cooker_acoustics::cook_prefab(
        &source.source_path,
        &manifest.name,
        manifest.materials.as_str(),
        &materials,
        &states,
    )?;
    let bytes = prefab.to_bytes()?;
    Ok(CookedAcoustic {
        source_hash: Some(blake3::hash(&bytes)),
        bytes: Bytes::from(bytes),
    })
}

pub(crate) fn cook_simulation(
    source: &LoadedAsset,
    manifest: &AcousticSimulationManifest,
    cooked: &BTreeMap<AssetId, CookedAsset>,
) -> anyhow::Result<CookedAcoustic> {
    let materials = material_definitions(&manifest.materials, cooked)?;
    let topology = cooked
        .get(&manifest.topology)
        .with_context(|| format!("acoustic topology `{}` was not cooked", manifest.topology))?;
    let topology = blackflower_acoustics::AcousticTopology::from_bytes(&topology.bytes)?;
    let simulation = blackflower_cooker_acoustics::cook_simulation_scene(
        &source.source_path,
        manifest.materials.as_str(),
        manifest.topology.as_str(),
        &materials,
        &topology,
    )?;
    let bytes = simulation.to_bytes()?;
    Ok(CookedAcoustic {
        source_hash: Some(blake3::hash(&bytes)),
        bytes: Bytes::from(bytes),
    })
}

#[allow(
    clippy::too_many_lines,
    reason = "emission cooking keeps strict media-kind validation and deterministic profile derivation together"
)]
pub(crate) fn cook_emission(
    manifest: &AcousticEmissionProfileManifest,
    repository: &Repository,
    profile: &CookingProfile,
) -> anyhow::Result<CookedAcoustic> {
    let media = repository
        .assets
        .get(&manifest.media)
        .with_context(|| format!("cook-time media `{}` does not exist", manifest.media))?;
    let extension = media
        .source_path
        .extension()
        .and_then(|value| value.to_str())
        .context("audio source extension is not UTF-8")?;
    let cooked_media = match &media.manifest.source {
        AssetSource::AudioClip(clip) => blackflower_audio_media::cook_clip(
            extension,
            &media.source_bytes,
            clip.loop_region
                .map(|region| {
                    blackflower_audio_media::LoopRegion::new(region.start_frame, region.end_frame)
                })
                .transpose()?,
        )?,
        AssetSource::AudioStream(_) => {
            blackflower_audio_media::cook_stream(extension, &media.source_bytes, profile.audio)?
        }
        AssetSource::Blob(_)
        | AssetSource::Luau(_)
        | AssetSource::Shader(_)
        | AssetSource::Texture(_)
        | AssetSource::Mesh(_)
        | AssetSource::Model(_)
        | AssetSource::Volume(_)
        | AssetSource::Skeleton(_)
        | AssetSource::Animation(_)
        | AssetSource::Navigation(_)
        | AssetSource::SoundEvent(_)
        | AssetSource::AcousticScene(_)
        | AssetSource::AcousticProbes(_)
        | AssetSource::Acoustic(_)
        | AssetSource::AcousticMaterials(_)
        | AssetSource::AcousticTopology(_)
        | AssetSource::AcousticPrefab(_)
        | AssetSource::AcousticSimulation(_)
        | AssetSource::AcousticEmission(_) => {
            anyhow::bail!("emission profile media `{}` is not audio", manifest.media)
        }
    };
    let source_hash = blake3::hash(&cooked_media);
    let emission = blackflower_cooker_acoustics::cook_emission_profile(
        Bytes::from(cooked_media),
        manifest.client_event_id,
        manifest.reference_spl_db,
        manifest.directivity,
        sound_class(manifest.class),
    )?;
    Ok(CookedAcoustic {
        bytes: Bytes::from(emission.to_bytes()?),
        source_hash: Some(source_hash),
    })
}

fn material_definitions(
    id: &AssetId,
    cooked: &BTreeMap<AssetId, CookedAsset>,
) -> anyhow::Result<Vec<blackflower_cooker_acoustics::AcousticMaterialDefinition>> {
    let asset = cooked
        .get(id)
        .with_context(|| format!("acoustic material library `{id}` was not cooked"))?;
    let library = blackflower_acoustics::AcousticMaterialLibrary::from_bytes(&asset.bytes)?;
    library
        .materials()
        .iter()
        .map(|material| {
            blackflower_cooker_acoustics::AcousticMaterialDefinition::new(
                material.id.clone(),
                material.absorption.0.map(dequantize),
                dequantize(material.scattering_q16),
                material.transmission.0.map(dequantize),
            )
            .map_err(anyhow::Error::from)
        })
        .collect()
}

fn dequantize(value: u16) -> f32 {
    f32::from(value) / f32::from(u16::MAX)
}

const fn sound_class(value: AcousticSoundClassManifest) -> blackflower_acoustics::SoundClass {
    match value {
        AcousticSoundClassManifest::Footstep => blackflower_acoustics::SoundClass::Footstep,
        AcousticSoundClassManifest::Gunshot => blackflower_acoustics::SoundClass::Gunshot,
        AcousticSoundClassManifest::Voice => blackflower_acoustics::SoundClass::Voice,
        AcousticSoundClassManifest::Impact => blackflower_acoustics::SoundClass::Impact,
        AcousticSoundClassManifest::Explosion => blackflower_acoustics::SoundClass::Explosion,
        AcousticSoundClassManifest::Mechanical => blackflower_acoustics::SoundClass::Mechanical,
    }
}

pub(crate) fn platform_identity() -> String {
    format!("{}/{}", std::env::consts::OS, std::env::consts::ARCH)
}
