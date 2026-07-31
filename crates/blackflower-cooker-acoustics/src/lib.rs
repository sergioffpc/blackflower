#![doc = include_str!("../README.md")]

mod error;
mod geometry;

use std::path::Path;

use blackflower_audio_spatial::{
    AcousticMaterial, AcousticScene, Context, PathBakeSettings, ProbeBatch, ReflectionsBakeSettings,
};

pub use error::Error;

/// Versioned semantic recipe for Stage 8 acoustic cooking.
pub const COOKER_RECIPE: &str = "blackflower-static-acoustics-v1";
/// Pinned Steam Audio source revision.
pub const STEAM_AUDIO_REVISION: &str = "0da18255cca520771f363ee01f100572b39a308e";

/// Explicit material mapping referenced from glTF schema-1 extras.
#[derive(Debug, Clone)]
pub struct AcousticMaterialDefinition {
    id: String,
    material: AcousticMaterial,
}

impl AcousticMaterialDefinition {
    /// Create a named acoustic material with validated Steam Audio coefficients.
    pub fn new(
        id: impl Into<String>,
        absorption: [f32; 3],
        scattering: f32,
        transmission: [f32; 3],
    ) -> Result<Self, Error> {
        let id = id.into();
        if !portable_asset_id(&id) {
            return Err(Error::InvalidSource(
                "acoustic material ID is not portable".to_owned(),
            ));
        }
        Ok(Self {
            id,
            material: AcousticMaterial::new(absorption, scattering, transmission)?,
        })
    }

    /// Portable ID referenced from glTF material metadata.
    #[must_use]
    pub fn id(&self) -> &str {
        &self.id
    }

    pub(crate) const fn material(&self) -> AcousticMaterial {
        self.material
    }
}

/// Centralized bake-quality settings selected by the cooking profile.
#[derive(Debug, Clone, Copy, PartialEq)]
pub struct AcousticBakeProfile {
    /// Base reflections and parametric reverb settings.
    pub reflections: ReflectionsBakeSettings,
    /// Probe-to-probe pathing settings.
    pub pathing: PathBakeSettings,
}

/// Cooked scene plus a hash of the consumed acoustic result.
pub struct CookedScene {
    /// Runtime `.bfacscn` object.
    pub asset: AcousticScene,
    /// Deterministic hash excluding unrelated dynamic glTF content.
    pub source_hash: blake3::Hash,
}

/// Cooked probe batch plus a hash of the consumed acoustic result.
pub struct CookedProbeBatch {
    /// Runtime `.bfacprb` object.
    pub asset: ProbeBatch,
    /// Deterministic hash excluding unrelated dynamic glTF content.
    pub source_hash: blake3::Hash,
}

/// Import only `static` geometry, resolve explicit materials, and serialize a
/// committed Steam Audio scene.
pub fn cook_scene(
    path: &Path,
    materials: &[AcousticMaterialDefinition],
) -> Result<CookedScene, Error> {
    let imported = geometry::import_scene(path, materials)?;
    let mut context = Context::new()?;
    let mut scene = context.create_scene()?;
    let mut mesh = scene.create_static_mesh(
        &imported.vertices,
        &imported.triangles,
        &imported.material_indices,
        &imported.materials,
    )?;
    mesh.add();
    scene.commit();
    let asset = scene.to_acoustic_asset(
        u32::try_from(imported.vertices.len()).map_err(|_error| {
            Error::InvalidSource("acoustic vertex count exceeds u32".to_owned())
        })?,
        u32::try_from(imported.triangles.len()).map_err(|_error| {
            Error::InvalidSource("acoustic triangle count exceeds u32".to_owned())
        })?,
        u32::try_from(imported.materials.len()).map_err(|_error| {
            Error::InvalidSource("acoustic material count exceeds u32".to_owned())
        })?,
    )?;
    let source_hash = blake3::hash(asset.bytes());
    Ok(CookedScene { asset, source_hash })
}

/// Select an authored probe volume, generate uniform-floor probes, and bake
/// base reflections/reverb plus dynamic pathing.
pub fn cook_probe_batch(
    path: &Path,
    scene_asset: &AcousticScene,
    volume_id: &str,
    spacing_meters: f32,
    height_meters: f32,
    profile: AcousticBakeProfile,
) -> Result<CookedProbeBatch, Error> {
    let volume = geometry::import_probe_volume(path, volume_id)?;
    let mut context = Context::new()?;
    let scene = context.load_acoustic_scene(scene_asset)?;
    let asset = context.bake_uniform_floor_probe_batch(
        &scene,
        volume.zone,
        volume.transform,
        spacing_meters,
        height_meters,
        profile.reflections,
        profile.pathing,
    )?;
    let source_hash = blake3::hash(asset.bytes());
    Ok(CookedProbeBatch { asset, source_hash })
}

/// Verify that every environment zone is authored in the selected glTF scene.
pub fn validate_environment_zones(path: &Path, zone_ids: &[String]) -> Result<(), Error> {
    let authored = geometry::import_zone_ids(path)?;
    for zone in zone_ids {
        if !authored.contains(zone) {
            return Err(Error::InvalidSource(format!(
                "acoustic environment references missing authored zone `{zone}`"
            )));
        }
    }
    Ok(())
}

fn portable_asset_id(value: &str) -> bool {
    !value.is_empty()
        && value.len() <= 255
        && value.is_ascii()
        && value.split('/').all(|segment| {
            !segment.is_empty()
                && !matches!(segment, "." | "..")
                && segment.bytes().all(|byte| {
                    byte.is_ascii_lowercase()
                        || byte.is_ascii_digit()
                        || matches!(byte, b'.' | b'_' | b'-')
                })
        })
}
