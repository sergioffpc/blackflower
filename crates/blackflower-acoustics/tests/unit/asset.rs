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
    assert_toml_payload(&first)?;
    assert_eq!(AcousticMaterialLibrary::from_bytes(&first)?, library);
    let mut corrupt = first;
    let last = corrupt.len() - 1;
    corrupt[last] ^= 1;
    assert!(AcousticMaterialLibrary::from_bytes(&corrupt).is_err());
    Ok(())
}

#[test]
fn unknown_schema_is_rejected() -> Result<(), Error> {
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
    bytes[8..12].copy_from_slice(&u32::MAX.to_le_bytes());
    assert!(AcousticEmissionProfile::from_bytes(&bytes).is_err());
    Ok(())
}

#[test]
#[allow(
    clippy::too_many_lines,
    reason = "one format acceptance proof compares every acoustic container's canonical bytes"
)]
fn every_acoustic_container_round_trips_canonically() -> Result<(), Error> {
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
    assert_toml_payload(&topology_bytes)?;
    assert_eq!(AcousticTopology::from_bytes(&topology_bytes)?, topology);

    let prefab = AcousticPrefab::new(
        "removed".to_owned(),
        "materials".to_owned(),
        vec![PrefabState {
            id: 0,
            name: "removed".to_owned(),
            triangles: Vec::new(),
        }],
    )?;
    let prefab_bytes = prefab.to_bytes()?;
    assert_toml_payload(&prefab_bytes)?;
    assert_eq!(AcousticPrefab::from_bytes(&prefab_bytes)?, prefab);

    let simulation = AcousticSimulationScene::new(AcousticSimulationScene {
        materials: "materials".to_owned(),
        topology: "topology".to_owned(),
        triangles: Vec::new(),
        paths: Vec::new(),
        zones: vec![ZoneResponse {
            zone: 1,
            late_gain: BandEnergy([40_000; 3]),
            decay_ms: 200,
        }],
    })?;
    let simulation_bytes = simulation.to_bytes()?;
    assert_toml_payload(&simulation_bytes)?;
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
    assert_toml_payload(&profile_bytes)?;
    assert_eq!(
        AcousticEmissionProfile::from_bytes(&profile_bytes)?,
        profile
    );
    Ok(())
}

fn assert_toml_payload(container: &[u8]) -> Result<(), Error> {
    let payload = container
        .get(HEADER_BYTES..)
        .ok_or(Error::InvalidField("acoustic container payload"))?;
    let text = std::str::from_utf8(payload)
        .map_err(|_error| Error::InvalidField("acoustic TOML payload"))?;
    let _document: toml::Table =
        toml::from_str(text).map_err(|_error| Error::InvalidField("acoustic TOML payload"))?;
    assert!(!text.trim_start().starts_with('{'));
    Ok(())
}
