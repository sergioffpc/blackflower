#![doc = include_str!("../README.md")]

mod error;
mod geometry;

use std::path::Path;

use blackflower_audio_spatial::{
    AcousticMaterial, AcousticScene, Context, PathBakeSettings, ProbeBatch, ReflectionsBakeSettings,
};

pub use error::Error;

/// Versioned semantic recipe for static acoustic cooking.
pub const COOKER_RECIPE: &str = "blackflower-static-acoustics-v1";
/// Versioned semantic recipe for authoritative acoustic cooking.
pub const AUTHORITATIVE_COOKER_RECIPE: &str =
    "blackflower-authoritative-acoustics-v1;mm;q0.16;20ms";
/// Pinned Steam Audio source revision.
pub const STEAM_AUDIO_REVISION: &str = "0da18255cca520771f363ee01f100572b39a308e";

/// Explicit material mapping referenced from glTF schema-1 extras.
#[derive(Debug, Clone)]
pub struct AcousticMaterialDefinition {
    id: String,
    material: AcousticMaterial,
    absorption: [f32; 3],
    scattering: f32,
    transmission: [f32; 3],
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
            absorption,
            scattering,
            transmission,
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

/// Explicit instance selected by an `.bfactpl` manifest.
#[derive(Debug, Clone)]
pub struct AcousticPrefabInstanceDefinition {
    /// Stable instance ID.
    pub id: u32,
    /// Referenced prefab asset ID.
    pub prefab: String,
    /// Default finite state.
    pub default_state: u32,
    /// Authored zone-volume identifiers touched by the instance.
    pub zones: Vec<String>,
}

/// One prefab state selected from a dynamic glTF node, or `removed` when node is absent.
#[derive(Debug, Clone)]
pub struct AcousticPrefabStateDefinition {
    /// Stable state ID.
    pub id: u32,
    /// Portable state name.
    pub name: String,
    /// Dynamic geometry node identifier; `None` means removed.
    pub node: Option<String>,
}

/// Build canonical shared `.bfacmat` bytes from the central material definition.
pub fn cook_material_library(
    definitions: &[AcousticMaterialDefinition],
) -> Result<blackflower_acoustics::AcousticMaterialLibrary, Error> {
    let materials = definitions
        .iter()
        .map(|definition| {
            Ok(blackflower_acoustics::AcousticMaterial {
                id: definition.id.clone(),
                absorption: quantize_bands(definition.absorption)?,
                scattering_q16: quantize_fraction(definition.scattering)?,
                transmission: quantize_bands(definition.transmission)?,
            })
        })
        .collect::<Result<Vec<_>, Error>>()?;
    blackflower_acoustics::AcousticMaterialLibrary::new(materials).map_err(Error::from)
}

/// Cook authored `zone_volume` and `portal` metadata into shared `.bfactpl`.
#[allow(
    clippy::too_many_lines,
    reason = "topology cooking keeps stable ID generation and all authored cross-reference checks together"
)]
pub fn cook_topology(
    path: &Path,
    instances: &[AcousticPrefabInstanceDefinition],
) -> Result<blackflower_acoustics::AcousticTopology, Error> {
    let imported = geometry::import_topology(path)?;
    let mut ids = std::collections::BTreeSet::new();
    let zones = imported
        .zones
        .iter()
        .map(|zone| {
            let id = stable_id(b"zone", &zone.name);
            if id == 0 || !ids.insert(id) {
                return Err(Error::InvalidSource(format!(
                    "acoustic zone ID collision for `{}`",
                    zone.name
                )));
            }
            Ok(blackflower_acoustics::AcousticZoneVolume {
                id,
                name: zone.name.clone(),
                bounds: zone.bounds,
            })
        })
        .collect::<Result<Vec<_>, Error>>()?;
    let zone_ids = zones
        .iter()
        .map(|zone| (zone.name.as_str(), zone.id))
        .collect::<std::collections::BTreeMap<_, _>>();
    ids.clear();
    let portals = imported
        .portals
        .iter()
        .map(|portal| {
            let id = stable_id(b"portal", &portal.name);
            if id == 0 || !ids.insert(id) {
                return Err(Error::InvalidSource(format!(
                    "acoustic portal ID collision for `{}`",
                    portal.name
                )));
            }
            Ok(blackflower_acoustics::AcousticPortal {
                id,
                zone_a: *zone_ids.get(portal.zone_a.as_str()).ok_or_else(|| {
                    Error::InvalidSource(format!("missing zone `{}`", portal.zone_a))
                })?,
                zone_b: *zone_ids.get(portal.zone_b.as_str()).ok_or_else(|| {
                    Error::InvalidSource(format!("missing zone `{}`", portal.zone_b))
                })?,
                center: portal.center,
                default_open_q16: u16::MAX,
                instance_id: instances
                    .iter()
                    .filter(|instance| {
                        instance.zones.iter().any(|zone| zone == &portal.zone_a)
                            && instance.zones.iter().any(|zone| zone == &portal.zone_b)
                    })
                    .map(|instance| instance.id)
                    .min(),
            })
        })
        .collect::<Result<Vec<_>, Error>>()?;
    let instances = instances
        .iter()
        .map(|instance| {
            let mut zones = instance
                .zones
                .iter()
                .map(|zone| {
                    zone_ids.get(zone.as_str()).copied().ok_or_else(|| {
                        Error::InvalidSource(format!("missing instance zone `{zone}`"))
                    })
                })
                .collect::<Result<Vec<_>, Error>>()?;
            zones.sort_unstable();
            zones.dedup();
            Ok(blackflower_acoustics::AcousticPrefabInstance {
                id: instance.id,
                prefab: instance.prefab.clone(),
                default_state: instance.default_state,
                zones,
            })
        })
        .collect::<Result<Vec<_>, Error>>()?;
    blackflower_acoustics::AcousticTopology::new(zones, portals, instances).map_err(Error::from)
}

/// Cook one explicitly selected rigid prefab with finite states into `.bfacpfb`.
pub fn cook_prefab(
    path: &Path,
    name: &str,
    material_asset: &str,
    materials: &[AcousticMaterialDefinition],
    states: &[AcousticPrefabStateDefinition],
) -> Result<blackflower_acoustics::AcousticPrefab, Error> {
    let states = states
        .iter()
        .map(|state| {
            let triangles = state.node.as_deref().map_or_else(
                || Ok(Vec::new()),
                |node| {
                    geometry::import_selected_geometry(path, materials, node)
                        .and_then(|scene| quantized_triangles(&scene))
                },
            )?;
            Ok(blackflower_acoustics::PrefabState {
                id: state.id,
                name: state.name.clone(),
                triangles,
            })
        })
        .collect::<Result<Vec<_>, Error>>()?;
    blackflower_acoustics::AcousticPrefab::new(name.to_owned(), material_asset.to_owned(), states)
        .map_err(Error::from)
}

/// Cook static geometry plus topology-derived path edges into `.bfacsim`.
pub fn cook_simulation_scene(
    path: &Path,
    material_asset: &str,
    topology_asset: &str,
    materials: &[AcousticMaterialDefinition],
    topology: &blackflower_acoustics::AcousticTopology,
) -> Result<blackflower_acoustics::AcousticSimulationScene, Error> {
    let imported = geometry::import_scene(path, materials)?;
    let triangles = quantized_triangles(&imported)?;
    let paths = topology
        .portals()
        .iter()
        .map(|portal| blackflower_acoustics::ProbePathEdge {
            zone_a: portal.zone_a,
            zone_b: portal.zone_b,
            length_mm: 1_000,
            gain: blackflower_acoustics::BandEnergy::UNITY,
        })
        .collect();
    let zones = topology
        .zones()
        .iter()
        .map(|zone| blackflower_acoustics::ZoneResponse {
            zone: zone.id,
            late_gain: blackflower_acoustics::BandEnergy::UNITY,
            decay_ms: 200,
        })
        .collect();
    blackflower_acoustics::AcousticSimulationScene::new(
        blackflower_acoustics::AcousticSimulationScene {
            materials: material_asset.to_owned(),
            topology: topology_asset.to_owned(),
            triangles,
            paths,
            zones,
        },
    )
    .map_err(Error::from)
}

/// Analyze cooked 48 kHz media into deterministic 20 ms `.bfacprf` frames.
pub fn cook_emission_profile(
    cooked_media: bytes::Bytes,
    client_event_id: u32,
    reference_spl_db: f32,
    directivity: f32,
    class: blackflower_acoustics::SoundClass,
) -> Result<blackflower_acoustics::AcousticEmissionProfile, Error> {
    let samples = decode_cooked_mono(cooked_media)?;
    let frames = spectral_frames(&samples);
    blackflower_acoustics::AcousticEmissionProfile::new(
        blackflower_acoustics::AcousticEmissionProfile {
            client_event_id,
            reference_spl_db_q8: quantize_db(reference_spl_db)?,
            directivity_q16: quantize_fraction(directivity)?,
            class,
            frames,
        },
    )
    .map_err(Error::from)
}

fn quantized_triangles(
    scene: &geometry::ImportedScene,
) -> Result<Vec<blackflower_acoustics::QuantizedTriangle>, Error> {
    scene
        .triangles
        .iter()
        .zip(&scene.material_indices)
        .map(|(triangle, material)| {
            let indices = triangle.indices();
            let vertex = |index: u32| {
                let point = scene
                    .vertices
                    .get(usize::try_from(index).unwrap_or(usize::MAX))
                    .copied()
                    .ok_or_else(|| {
                        Error::InvalidSource("invalid acoustic triangle index".to_owned())
                    })?;
                geometry::quantize_position(glam::Vec3::from(point))
            };
            blackflower_acoustics::QuantizedTriangle::new(
                [
                    vertex(indices[0])?,
                    vertex(indices[1])?,
                    vertex(indices[2])?,
                ],
                u16::try_from(*material).map_err(|_error| {
                    Error::InvalidSource("acoustic material index exceeds u16".to_owned())
                })?,
            )
            .map_err(Error::from)
        })
        .collect()
}

fn stable_id(domain: &[u8], value: &str) -> u32 {
    let mut hasher = blake3::Hasher::new();
    hasher.update(b"blackflower.acoustic-id.v1");
    hasher.update(domain);
    hasher.update(value.as_bytes());
    let bytes: [u8; 4] = hasher.finalize().as_bytes()[..4]
        .try_into()
        .unwrap_or([0; 4]);
    u32::from_le_bytes(bytes)
}

#[allow(
    clippy::cast_possible_truncation,
    clippy::cast_sign_loss,
    reason = "validated unit coefficients are intentionally quantized to Q0.16"
)]
fn quantize_fraction(value: f32) -> Result<u16, Error> {
    if !value.is_finite() || !(0.0..=1.0).contains(&value) {
        return Err(Error::InvalidSource(
            "acoustic coefficient is not a unit fraction".to_owned(),
        ));
    }
    Ok((f64::from(value) * f64::from(u16::MAX)).round() as u16)
}

fn quantize_bands(values: [f32; 3]) -> Result<blackflower_acoustics::BandEnergy, Error> {
    Ok(blackflower_acoustics::BandEnergy([
        quantize_fraction(values[0])?,
        quantize_fraction(values[1])?,
        quantize_fraction(values[2])?,
    ]))
}

#[allow(
    clippy::cast_possible_truncation,
    reason = "validated authored SPL is intentionally quantized to signed Q8.8"
)]
fn quantize_db(value: f32) -> Result<i32, Error> {
    if !value.is_finite() || !(-120.0..=240.0).contains(&value) {
        return Err(Error::InvalidSource("reference SPL is invalid".to_owned()));
    }
    Ok((f64::from(value) * 256.0).round() as i32)
}

fn decode_cooked_mono(bytes: bytes::Bytes) -> Result<Vec<i16>, Error> {
    if let Ok(clip) = blackflower_audio_media::AudioClip::from_bytes(bytes.clone()) {
        let channels = usize::from(clip.channels());
        return Ok(clip
            .samples()
            .chunks_exact(channels)
            .map(|frame| {
                let total = frame.iter().map(|sample| i32::from(*sample)).sum::<i32>();
                i16::try_from(total / i32::try_from(channels).unwrap_or(1)).unwrap_or(0)
            })
            .collect());
    }
    let stream = blackflower_audio_media::AudioStream::from_bytes(bytes)?;
    let mut decoder = stream.decoder()?;
    let mut samples = Vec::with_capacity(stream.frame_count());
    loop {
        let frames = decoder.decode()?;
        if frames.is_empty() {
            break;
        }
        samples.extend(
            frames
                .into_iter()
                .map(|frame| quantize_pcm((frame.left + frame.right) * 0.5)),
        );
    }
    Ok(samples)
}

#[allow(
    clippy::cast_possible_truncation,
    reason = "decoded finite PCM is clamped then intentionally quantized to i16"
)]
fn quantize_pcm(sample: f32) -> i16 {
    (f64::from(sample.clamp(-1.0, 1.0)) * f64::from(i16::MAX)).round() as i16
}

fn spectral_frames(samples: &[i16]) -> Vec<blackflower_acoustics::SpectralEnvelopeFrame> {
    const FRAME: usize = 960;
    samples
        .chunks(FRAME)
        .map(|frame| {
            let mut square_sum = 0_u128;
            let mut low = 0_u64;
            let mut high = 0_u64;
            for (index, sample) in frame.iter().enumerate() {
                let value = i64::from(*sample);
                square_sum = square_sum.saturating_add(u128::from(value.unsigned_abs()).pow(2));
                let previous = i64::from(*frame.get(index.saturating_sub(1)).unwrap_or(sample));
                high = high.saturating_add(value.saturating_sub(previous).unsigned_abs());
                let begin = index.saturating_sub(3);
                let window = &frame[begin..=index];
                let average = window.iter().map(|sample| i64::from(*sample)).sum::<i64>()
                    / i64::try_from(window.len()).unwrap_or(1);
                low = low.saturating_add(average.unsigned_abs());
            }
            let count = u128::try_from(frame.len()).unwrap_or(1).max(1);
            let rms = integer_sqrt(square_sum / count).min(32_767);
            let total = frame
                .iter()
                .map(|sample| i64::from(*sample).unsigned_abs())
                .sum::<u64>()
                .max(1);
            let low = low.min(total);
            let high = (high / 2).min(total.saturating_sub(low));
            let mid = total.saturating_sub(low).saturating_sub(high);
            blackflower_acoustics::SpectralEnvelopeFrame {
                amplitude_q16: u16::try_from(rms.saturating_mul(u64::from(u16::MAX)) / 32_767)
                    .unwrap_or(u16::MAX),
                bands: blackflower_acoustics::BandEnergy([
                    normalize_band(low, total),
                    normalize_band(mid, total),
                    normalize_band(high, total),
                ]),
            }
        })
        .collect()
}

fn normalize_band(value: u64, total: u64) -> u16 {
    u16::try_from(value.saturating_mul(u64::from(u16::MAX)) / total.max(1)).unwrap_or(u16::MAX)
}

fn integer_sqrt(value: u128) -> u64 {
    let mut low = 0_u128;
    let mut high = value.min(u128::from(u64::MAX));
    while low <= high {
        let middle = (low + high) / 2;
        let square = middle.saturating_mul(middle);
        if square <= value {
            low = middle.saturating_add(1);
        } else if middle == 0 {
            break;
        } else {
            high = middle - 1;
        }
    }
    u64::try_from(high).unwrap_or(u64::MAX)
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
    let mut scene = context.create_serializable_scene()?;
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
