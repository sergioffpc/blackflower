use std::collections::BTreeMap;

use anyhow::Context;
use blackflower_assets::{AssetId, Bytes};
use blackflower_audio_spatial::{AcousticEnvironment, AcousticScene, AcousticZone, ProbeBatch};

use crate::asset_cooker::CookedAsset;
use crate::manifest::{
    AcousticManifest, AcousticProbesManifest, AcousticSceneManifest, LoadedAsset,
};
use crate::profile::AcousticsProfile;

pub(crate) struct CookedAcoustic {
    pub(crate) bytes: Bytes,
    pub(crate) source_hash: Option<blake3::Hash>,
}

pub(crate) fn cook_scene(
    source: &LoadedAsset,
    manifest: &AcousticSceneManifest,
) -> anyhow::Result<CookedAcoustic> {
    let materials = manifest
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
        .collect::<Result<Vec<_>, _>>()
        .context("invalid acoustic material table")?;
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
    let environment = AcousticEnvironment::new(zones)?;
    Ok(CookedAcoustic {
        bytes: Bytes::copy_from_slice(environment.bytes()),
        source_hash: None,
    })
}

pub(crate) fn platform_identity() -> String {
    format!("{}/{}", std::env::consts::OS, std::env::consts::ARCH)
}
