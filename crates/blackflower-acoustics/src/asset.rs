use std::collections::BTreeSet;

use serde::de::DeserializeOwned;
use serde::{Deserialize, Serialize};

use crate::{AabbMm, AcousticBvh, BandEnergy, Error, PositionMm, QuantizedTriangle, SoundClass};

/// Current schema for all Stage 9 authoritative acoustic containers.
pub const ACOUSTIC_ASSET_SCHEMA: u32 = 1;

const MATERIAL_MAGIC: &[u8; 8] = b"BFACMAT\0";
const TOPOLOGY_MAGIC: &[u8; 8] = b"BFACTPL\0";
const PREFAB_MAGIC: &[u8; 8] = b"BFACPFB\0";
const SIMULATION_MAGIC: &[u8; 8] = b"BFACSIM\0";
const EMISSION_MAGIC: &[u8; 8] = b"BFACPRF\0";
const HEADER_BYTES: usize = 8 + 4 + 8 + 32;
const MAX_ASSET_BYTES: usize = 256 * 1024 * 1024;

/// Canonical frequency response for one authored material.
#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
#[serde(deny_unknown_fields)]
pub struct AcousticMaterial {
    /// Portable material ID referenced by geometry.
    pub id: String,
    /// Fraction absorbed by low, mid, and high bands in Q0.16.
    pub absorption: BandEnergy,
    /// Diffuse scattering fraction in Q0.16.
    pub scattering_q16: u16,
    /// Fraction transmitted by low, mid, and high bands in Q0.16.
    pub transmission: BandEnergy,
}

/// Shared `.bfacmat` source of truth consumed by both Rust and Steam cookers.
#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
#[serde(deny_unknown_fields)]
pub struct AcousticMaterialLibrary {
    materials: Vec<AcousticMaterial>,
}

impl AcousticMaterialLibrary {
    /// Sort and validate one material table.
    pub fn new(mut materials: Vec<AcousticMaterial>) -> Result<Self, Error> {
        if materials.is_empty() {
            return Err(Error::InvalidField("acoustic materials"));
        }
        materials.sort_by(|left, right| left.id.cmp(&right.id));
        validate_unique_text(materials.iter().map(|material| material.id.as_str()))?;
        Ok(Self { materials })
    }

    /// Decode and validate `.bfacmat`.
    pub fn from_bytes(bytes: &[u8]) -> Result<Self, Error> {
        let value: Self = decode_container(MATERIAL_MAGIC, "acoustic material library", bytes)?;
        Self::new(value.materials)
    }

    /// Encode canonical `.bfacmat` bytes.
    pub fn to_bytes(&self) -> Result<Vec<u8>, Error> {
        encode_container(MATERIAL_MAGIC, "acoustic material library", self)
    }

    /// Canonically ordered material definitions.
    #[must_use]
    pub fn materials(&self) -> &[AcousticMaterial] {
        &self.materials
    }
}

/// One authored zone volume.
#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
#[serde(deny_unknown_fields)]
pub struct AcousticZoneVolume {
    /// Stable zone ID.
    pub id: u32,
    /// Portable authored name.
    pub name: String,
    /// Quantized world bounds.
    pub bounds: AabbMm,
}

/// One connection between two authored zones.
#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
#[serde(deny_unknown_fields)]
pub struct AcousticPortal {
    /// Stable portal ID.
    pub id: u32,
    /// First adjacent zone.
    pub zone_a: u32,
    /// Second adjacent zone.
    pub zone_b: u32,
    /// Representative portal center used for path length.
    pub center: PositionMm,
    /// Default openness in Q0.16.
    pub default_open_q16: u16,
    /// Optional dynamic instance whose committed state drives this portal.
    pub instance_id: Option<u32>,
}

/// One explicitly selected rigid acoustic prefab instance.
#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
#[serde(deny_unknown_fields)]
pub struct AcousticPrefabInstance {
    /// Stable instance ID.
    pub id: u32,
    /// Referenced `.bfacpfb` asset ID.
    pub prefab: String,
    /// Default state ID.
    pub default_state: u32,
    /// Zones touched by this instance.
    pub zones: Vec<u32>,
}

/// Shared `.bfactpl` zone, portal, instance, and state topology.
#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
#[serde(deny_unknown_fields)]
pub struct AcousticTopology {
    zones: Vec<AcousticZoneVolume>,
    portals: Vec<AcousticPortal>,
    instances: Vec<AcousticPrefabInstance>,
}

impl AcousticTopology {
    /// Canonicalize and validate topology references.
    #[allow(
        clippy::too_many_lines,
        reason = "canonical topology validation keeps cross-reference checks in one constructor"
    )]
    pub fn new(
        mut zones: Vec<AcousticZoneVolume>,
        mut portals: Vec<AcousticPortal>,
        mut instances: Vec<AcousticPrefabInstance>,
    ) -> Result<Self, Error> {
        if zones.is_empty() {
            return Err(Error::InvalidField("acoustic zones"));
        }
        zones.sort_by_key(|zone| zone.id);
        portals.sort_by_key(|portal| portal.id);
        instances.sort_by_key(|instance| instance.id);
        validate_unique_ids(zones.iter().map(|zone| zone.id), "zone")?;
        validate_unique_ids(portals.iter().map(|portal| portal.id), "portal")?;
        validate_unique_ids(instances.iter().map(|instance| instance.id), "instance")?;
        validate_unique_text(zones.iter().map(|zone| zone.name.as_str()))?;
        let zone_ids = zones.iter().map(|zone| zone.id).collect::<BTreeSet<_>>();
        let instance_ids = instances
            .iter()
            .map(|instance| instance.id)
            .collect::<BTreeSet<_>>();
        for portal in &portals {
            if portal.zone_a == portal.zone_b
                || !zone_ids.contains(&portal.zone_a)
                || !zone_ids.contains(&portal.zone_b)
            {
                return Err(Error::MissingReference(format!(
                    "portal {} zone",
                    portal.id
                )));
            }
            if portal
                .instance_id
                .is_some_and(|instance| !instance_ids.contains(&instance))
            {
                return Err(Error::MissingReference(format!(
                    "portal {} instance",
                    portal.id
                )));
            }
        }
        for instance in &mut instances {
            validate_text(&instance.prefab)?;
            instance.zones.sort_unstable();
            instance.zones.dedup();
            if instance.zones.is_empty()
                || instance.zones.iter().any(|zone| !zone_ids.contains(zone))
            {
                return Err(Error::MissingReference(format!(
                    "instance {} zone",
                    instance.id
                )));
            }
        }
        Ok(Self {
            zones,
            portals,
            instances,
        })
    }

    /// Decode and validate `.bfactpl`.
    pub fn from_bytes(bytes: &[u8]) -> Result<Self, Error> {
        let value: Self = decode_container(TOPOLOGY_MAGIC, "acoustic topology", bytes)?;
        Self::new(value.zones, value.portals, value.instances)
    }

    /// Encode canonical `.bfactpl` bytes.
    pub fn to_bytes(&self) -> Result<Vec<u8>, Error> {
        encode_container(TOPOLOGY_MAGIC, "acoustic topology", self)
    }

    /// Authored zone volumes.
    #[must_use]
    pub fn zones(&self) -> &[AcousticZoneVolume] {
        &self.zones
    }

    /// Zone portals.
    #[must_use]
    pub fn portals(&self) -> &[AcousticPortal] {
        &self.portals
    }

    /// Dynamic rigid instances.
    #[must_use]
    pub fn instances(&self) -> &[AcousticPrefabInstance] {
        &self.instances
    }
}

/// One door or destructible geometry state; an empty triangle list is `removed`.
#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
#[serde(deny_unknown_fields)]
pub struct PrefabState {
    /// Stable state ID selected by committed gameplay state.
    pub id: u32,
    /// Portable authored state name.
    pub name: String,
    /// Quantized local-space acoustic triangles.
    pub triangles: Vec<QuantizedTriangle>,
    /// Cooker-built local BVH.
    pub bvh: AcousticBvh,
}

/// Shared `.bfacpfb` rigid geometry and finite state variants.
#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
#[serde(deny_unknown_fields)]
pub struct AcousticPrefab {
    /// Portable prefab name.
    pub name: String,
    /// Referenced material library asset ID.
    pub materials: String,
    states: Vec<PrefabState>,
}

impl AcousticPrefab {
    /// Build and validate all state BVHs.
    pub fn new(
        name: String,
        materials: String,
        mut states: Vec<PrefabState>,
    ) -> Result<Self, Error> {
        validate_text(&name)?;
        validate_text(&materials)?;
        if states.is_empty() {
            return Err(Error::InvalidField("prefab states"));
        }
        states.sort_by_key(|state| state.id);
        validate_unique_ids(states.iter().map(|state| state.id), "prefab state")?;
        validate_unique_text(states.iter().map(|state| state.name.as_str()))?;
        for state in &mut states {
            let expected = AcousticBvh::build(&state.triangles)?;
            if state.bvh.nodes.is_empty() && !state.triangles.is_empty() {
                state.bvh = expected;
            } else if state.bvh != expected {
                return Err(Error::InvalidField("prefab BVH"));
            }
        }
        Ok(Self {
            name,
            materials,
            states,
        })
    }

    /// Decode and validate `.bfacpfb`.
    pub fn from_bytes(bytes: &[u8]) -> Result<Self, Error> {
        let value: Self = decode_container(PREFAB_MAGIC, "acoustic prefab", bytes)?;
        Self::new(value.name, value.materials, value.states)
    }

    /// Encode canonical `.bfacpfb` bytes.
    pub fn to_bytes(&self) -> Result<Vec<u8>, Error> {
        encode_container(PREFAB_MAGIC, "acoustic prefab", self)
    }

    /// Canonically ordered finite states.
    #[must_use]
    pub fn states(&self) -> &[PrefabState] {
        &self.states
    }

    /// Find one finite state.
    #[must_use]
    pub fn state(&self, id: u32) -> Option<&PrefabState> {
        self.states
            .binary_search_by_key(&id, |state| state.id)
            .ok()
            .map(|index| &self.states[index])
    }
}

/// Baked stable path edge between authored zones or probes.
#[derive(Debug, Clone, Copy, PartialEq, Eq, Serialize, Deserialize)]
#[serde(deny_unknown_fields)]
pub struct ProbePathEdge {
    /// First zone.
    pub zone_a: u32,
    /// Second zone.
    pub zone_b: u32,
    /// Baked path length in millimetres.
    pub length_mm: u64,
    /// Baked low, mid, and high path gain.
    pub gain: BandEnergy,
}

/// Late response authored/baked for one zone.
#[derive(Debug, Clone, Copy, PartialEq, Eq, Serialize, Deserialize)]
#[serde(deny_unknown_fields)]
pub struct ZoneResponse {
    /// Stable zone ID.
    pub zone: u32,
    /// Late energy multiplier by band.
    pub late_gain: BandEnergy,
    /// Quantized late decay time in milliseconds.
    pub decay_ms: u16,
}

/// Simulation-only `.bfacsim` geometry, BVH, path graph, and zone response.
#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
#[serde(deny_unknown_fields)]
pub struct AcousticSimulationScene {
    /// Material library dependency.
    pub materials: String,
    /// Topology dependency.
    pub topology: String,
    /// Static quantized triangles.
    pub triangles: Vec<QuantizedTriangle>,
    /// Cooker-built static BVH.
    pub bvh: AcousticBvh,
    /// Canonically ordered path graph.
    pub paths: Vec<ProbePathEdge>,
    /// Canonically ordered late zone response.
    pub zones: Vec<ZoneResponse>,
}

impl AcousticSimulationScene {
    /// Validate and canonicalize one authoritative scene.
    pub fn new(mut value: Self) -> Result<Self, Error> {
        validate_text(&value.materials)?;
        validate_text(&value.topology)?;
        let expected = AcousticBvh::build(&value.triangles)?;
        if value.bvh.nodes.is_empty() && !value.triangles.is_empty() {
            value.bvh = expected;
        } else if value.bvh != expected {
            return Err(Error::InvalidField("simulation BVH"));
        }
        value
            .paths
            .sort_by_key(|edge| (edge.zone_a.min(edge.zone_b), edge.zone_a.max(edge.zone_b)));
        value.zones.sort_by_key(|zone| zone.zone);
        validate_unique_ids(value.zones.iter().map(|zone| zone.zone), "zone response")?;
        Ok(value)
    }

    /// Decode and validate `.bfacsim`.
    pub fn from_bytes(bytes: &[u8]) -> Result<Self, Error> {
        let value: Self = decode_container(SIMULATION_MAGIC, "acoustic simulation scene", bytes)?;
        Self::new(value)
    }

    /// Encode canonical `.bfacsim` bytes.
    pub fn to_bytes(&self) -> Result<Vec<u8>, Error> {
        encode_container(SIMULATION_MAGIC, "acoustic simulation scene", self)
    }
}

/// One deterministic 20 ms spectral frame derived from cooked 48 kHz media.
#[derive(Debug, Clone, Copy, PartialEq, Eq, Serialize, Deserialize)]
#[serde(deny_unknown_fields)]
pub struct SpectralEnvelopeFrame {
    /// Frame-relative RMS in Q0.16.
    pub amplitude_q16: u16,
    /// Normalized low, mid, and high distribution.
    pub bands: BandEnergy,
}

/// Simulation-only `.bfacprf` source strength and spectral envelope.
#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
#[serde(deny_unknown_fields)]
pub struct AcousticEmissionProfile {
    /// Stable client event ID.
    pub client_event_id: u32,
    /// Authored SPL at one metre in signed Q8.8 decibels.
    pub reference_spl_db_q8: i32,
    /// Source directivity in Q0.16, where zero is omnidirectional.
    pub directivity_q16: u16,
    /// Gameplay classification.
    pub class: SoundClass,
    /// Deterministic 20 ms frames.
    pub frames: Vec<SpectralEnvelopeFrame>,
}

impl AcousticEmissionProfile {
    /// Validate a non-empty 20 ms envelope.
    pub fn new(value: Self) -> Result<Self, Error> {
        if value.frames.is_empty() {
            return Err(Error::InvalidField("emission envelope"));
        }
        Ok(value)
    }

    /// Decode and validate `.bfacprf`.
    pub fn from_bytes(bytes: &[u8]) -> Result<Self, Error> {
        let value: Self = decode_container(EMISSION_MAGIC, "acoustic emission profile", bytes)?;
        Self::new(value)
    }

    /// Encode canonical `.bfacprf` bytes.
    pub fn to_bytes(&self) -> Result<Vec<u8>, Error> {
        encode_container(EMISSION_MAGIC, "acoustic emission profile", self)
    }
}

fn encode_container<T: Serialize>(
    magic: &[u8; 8],
    format: &'static str,
    value: &T,
) -> Result<Vec<u8>, Error> {
    let payload = toml::to_string(value)
        .map_err(|_error| invalid(format, "TOML encode failed"))?
        .into_bytes();
    if payload.is_empty() || payload.len() > MAX_ASSET_BYTES {
        return Err(invalid(format, "payload size is invalid"));
    }
    let mut bytes = Vec::with_capacity(HEADER_BYTES.saturating_add(payload.len()));
    bytes.extend_from_slice(magic);
    bytes.extend_from_slice(&ACOUSTIC_ASSET_SCHEMA.to_le_bytes());
    bytes.extend_from_slice(
        &u64::try_from(payload.len())
            .map_err(|_error| invalid(format, "payload size exceeds u64"))?
            .to_le_bytes(),
    );
    bytes.extend_from_slice(blake3::hash(&payload).as_bytes());
    bytes.extend_from_slice(&payload);
    Ok(bytes)
}

fn decode_container<T: DeserializeOwned + Serialize>(
    magic: &[u8; 8],
    format: &'static str,
    bytes: &[u8],
) -> Result<T, Error> {
    if bytes.len() < HEADER_BYTES || bytes.get(..8) != Some(magic) {
        return Err(invalid(format, "magic is invalid"));
    }
    let schema = read_u32(bytes, 8, format)?;
    if schema != ACOUSTIC_ASSET_SCHEMA {
        return Err(invalid(format, "schema is unsupported"));
    }
    let length = usize::try_from(read_u64(bytes, 12, format)?)
        .map_err(|_error| invalid(format, "payload length exceeds usize"))?;
    if length == 0 || length > MAX_ASSET_BYTES || bytes.len() != HEADER_BYTES.saturating_add(length)
    {
        return Err(invalid(format, "payload length is invalid"));
    }
    let checksum = bytes
        .get(20..52)
        .ok_or_else(|| invalid(format, "checksum is truncated"))?;
    let payload = bytes
        .get(HEADER_BYTES..)
        .ok_or_else(|| invalid(format, "payload is truncated"))?;
    if blake3::hash(payload).as_bytes() != checksum {
        return Err(invalid(format, "checksum does not match"));
    }
    let value: T =
        toml::from_slice(payload).map_err(|_error| invalid(format, "payload TOML is invalid"))?;
    let canonical = toml::to_string(&value)
        .map_err(|_error| invalid(format, "canonical TOML encode failed"))?;
    if canonical.as_bytes() != payload {
        return Err(invalid(format, "payload is not canonical"));
    }
    Ok(value)
}

fn read_u32(bytes: &[u8], offset: usize, format: &'static str) -> Result<u32, Error> {
    let value: [u8; 4] = bytes
        .get(offset..offset + 4)
        .ok_or_else(|| invalid(format, "integer is truncated"))?
        .try_into()
        .map_err(|_error| invalid(format, "integer is truncated"))?;
    Ok(u32::from_le_bytes(value))
}

fn read_u64(bytes: &[u8], offset: usize, format: &'static str) -> Result<u64, Error> {
    let value: [u8; 8] = bytes
        .get(offset..offset + 8)
        .ok_or_else(|| invalid(format, "integer is truncated"))?
        .try_into()
        .map_err(|_error| invalid(format, "integer is truncated"))?;
    Ok(u64::from_le_bytes(value))
}

fn validate_unique_ids(
    values: impl IntoIterator<Item = u32>,
    name: &'static str,
) -> Result<(), Error> {
    let mut previous = None;
    for value in values {
        if previous == Some(value) {
            return Err(Error::DuplicateIdentifier(format!("{name}:{value}")));
        }
        previous = Some(value);
    }
    Ok(())
}

fn validate_unique_text<'a>(values: impl IntoIterator<Item = &'a str>) -> Result<(), Error> {
    let mut previous: Option<&str> = None;
    for value in values {
        validate_text(value)?;
        if previous == Some(value) {
            return Err(Error::DuplicateIdentifier(value.to_owned()));
        }
        previous = Some(value);
    }
    Ok(())
}

fn validate_text(value: &str) -> Result<(), Error> {
    if value.is_empty()
        || value.len() > 255
        || !value.is_ascii()
        || value.trim() != value
        || value.chars().any(char::is_control)
    {
        Err(Error::InvalidField("portable acoustic text"))
    } else {
        Ok(())
    }
}

const fn invalid(format: &'static str, reason: &'static str) -> Error {
    Error::InvalidAsset { format, reason }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn material_container_is_deterministic_and_strict() -> Result<(), Error> {
        let library = AcousticMaterialLibrary::new(vec![AcousticMaterial {
            id: "concrete".to_owned(),
            absorption: BandEnergy([4_000, 6_000, 8_000]),
            scattering_q16: 10_000,
            transmission: BandEnergy([1_000, 500, 100]),
        }])?;
        let first = library.to_bytes()?;
        let second = library.to_bytes()?;
        assert_eq!(first, second);
        assert_eq!(AcousticMaterialLibrary::from_bytes(&first)?, library);
        let mut corrupt = first;
        let last = corrupt.len() - 1;
        corrupt[last] ^= 1;
        assert!(AcousticMaterialLibrary::from_bytes(&corrupt).is_err());
        Ok(())
    }

    #[test]
    fn v1_or_unknown_schema_is_rejected() -> Result<(), Error> {
        let profile = AcousticEmissionProfile::new(AcousticEmissionProfile {
            client_event_id: 7,
            reference_spl_db_q8: 80 * 256,
            directivity_q16: 0,
            class: SoundClass::Footstep,
            frames: vec![SpectralEnvelopeFrame {
                amplitude_q16: u16::MAX,
                bands: BandEnergy::UNITY,
            }],
        })?;
        let mut bytes = profile.to_bytes()?;
        bytes[8..12].copy_from_slice(&2_u32.to_le_bytes());
        assert!(AcousticEmissionProfile::from_bytes(&bytes).is_err());
        Ok(())
    }

    #[test]
    #[allow(
        clippy::too_many_lines,
        reason = "one format acceptance proof compares every Stage 9 container's canonical bytes"
    )]
    fn every_stage_nine_container_round_trips_canonically() -> Result<(), Error> {
        let topology = AcousticTopology::new(
            vec![AcousticZoneVolume {
                id: 1,
                name: "room".to_owned(),
                bounds: AabbMm::new(
                    PositionMm::new(-1_000, -1_000, -1_000),
                    PositionMm::new(1_000, 1_000, 1_000),
                )?,
            }],
            Vec::new(),
            Vec::new(),
        )?;
        let topology_bytes = topology.to_bytes()?;
        assert_eq!(AcousticTopology::from_bytes(&topology_bytes)?, topology);

        let prefab = AcousticPrefab::new(
            "removed".to_owned(),
            "materials".to_owned(),
            vec![PrefabState {
                id: 0,
                name: "removed".to_owned(),
                triangles: Vec::new(),
                bvh: AcousticBvh::build(&[])?,
            }],
        )?;
        let prefab_bytes = prefab.to_bytes()?;
        assert_eq!(AcousticPrefab::from_bytes(&prefab_bytes)?, prefab);

        let simulation = AcousticSimulationScene::new(AcousticSimulationScene {
            materials: "materials".to_owned(),
            topology: "topology".to_owned(),
            triangles: Vec::new(),
            bvh: AcousticBvh::build(&[])?,
            paths: Vec::new(),
            zones: vec![ZoneResponse {
                zone: 1,
                late_gain: BandEnergy([40_000; 3]),
                decay_ms: 200,
            }],
        })?;
        let simulation_bytes = simulation.to_bytes()?;
        assert_eq!(
            AcousticSimulationScene::from_bytes(&simulation_bytes)?,
            simulation
        );

        let profile = AcousticEmissionProfile::new(AcousticEmissionProfile {
            client_event_id: 7,
            reference_spl_db_q8: 80 * 256,
            directivity_q16: 0,
            class: SoundClass::Footstep,
            frames: vec![SpectralEnvelopeFrame {
                amplitude_q16: u16::MAX,
                bands: BandEnergy::UNITY,
            }],
        })?;
        let profile_bytes = profile.to_bytes()?;
        assert_eq!(
            AcousticEmissionProfile::from_bytes(&profile_bytes)?,
            profile
        );
        Ok(())
    }
}
