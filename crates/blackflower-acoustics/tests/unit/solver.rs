use crate::{
    AabbMm, AcousticMaterial, AcousticPortal, AcousticPrefabInstance, AcousticZoneVolume,
    EncodedVoice, ProbePathEdge, QuantizedTransform, SoundClass, VoiceStreamId, ZoneResponse,
};

use super::*;

#[allow(
    clippy::too_many_lines,
    reason = "solver tests share one complete two-room topology and material fixture"
)]
fn fixture() -> Result<AcousticWorld, Error> {
    let materials = AcousticMaterialLibrary::new(vec![
        AcousticMaterial {
            id: "breached".to_owned(),
            absorption: BandEnergy([5_000; 3]),
            scattering_q16: 1_000,
            transmission: BandEnergy([50_000, 40_000, 30_000]),
        },
        AcousticMaterial {
            id: "door".to_owned(),
            absorption: BandEnergy([20_000; 3]),
            scattering_q16: 1_000,
            transmission: BandEnergy([30_000, 8_000, 2_000]),
        },
    ])?;
    let topology = AcousticTopology::new(
        vec![
            AcousticZoneVolume {
                id: 1,
                name: "left".to_owned(),
                bounds: AabbMm::new(
                    PositionMm::new(-5_000, -2_000, -2_000),
                    PositionMm::new(0, 2_000, 2_000),
                )?,
            },
            AcousticZoneVolume {
                id: 2,
                name: "right".to_owned(),
                bounds: AabbMm::new(
                    PositionMm::new(1, -2_000, -2_000),
                    PositionMm::new(5_000, 2_000, 2_000),
                )?,
            },
        ],
        vec![AcousticPortal {
            id: 7,
            zone_a: 1,
            zone_b: 2,
            center: PositionMm::default(),
            default_open_q16: u16::MAX,
            instance_id: Some(10),
        }],
        vec![AcousticPrefabInstance {
            id: 10,
            prefab: "door".to_owned(),
            default_state: 0,
            zones: vec![1, 2],
        }],
    )?;
    let door = QuantizedTriangle::new(
        [
            PositionMm::new(0, -1_000, -1_000),
            PositionMm::new(0, 1_000, -1_000),
            PositionMm::new(0, 0, 1_000),
        ],
        1,
    )?;
    let breach = QuantizedTriangle::new(
        [
            PositionMm::new(0, -500, -500),
            PositionMm::new(0, 500, -500),
            PositionMm::new(0, 0, 500),
        ],
        0,
    )?;
    let prefab = AcousticPrefab::new(
        "door".to_owned(),
        "materials".to_owned(),
        vec![
            crate::PrefabState {
                id: 0,
                name: "open".to_owned(),
                triangles: Vec::new(),
            },
            crate::PrefabState {
                id: 1,
                name: "intact".to_owned(),
                triangles: vec![door],
            },
            crate::PrefabState {
                id: 2,
                name: "breached".to_owned(),
                triangles: vec![breach],
            },
            crate::PrefabState {
                id: 3,
                name: "removed".to_owned(),
                triangles: Vec::new(),
            },
        ],
    )?;
    let scene = AcousticSimulationScene::new(AcousticSimulationScene {
        materials: "materials".to_owned(),
        topology: "topology".to_owned(),
        triangles: Vec::new(),
        paths: vec![ProbePathEdge {
            zone_a: 1,
            zone_b: 2,
            length_mm: 2_000,
            gain: BandEnergy([60_000; 3]),
        }],
        zones: vec![ZoneResponse {
            zone: 1,
            late_gain: BandEnergy::UNITY,
            decay_ms: 200,
        }],
    })?;
    AcousticWorld::new(
        materials,
        topology,
        scene,
        vec![prefab],
        AcousticWorldSettings::default(),
    )
}

fn receiver(bot: bool) -> AcousticReceiver {
    AcousticReceiver {
        id: 2,
        position: PositionMm::new(2_000, 0, 0),
        zone: Some(2),
        threshold_db_q8: 10 * 256,
        masking_db_q8: 0,
        hearing: BandEnergy::UNITY,
        bot,
    }
}

fn emission(voice: bool) -> Result<SoundEmission, Error> {
    Ok(SoundEmission {
        id: 99,
        client_event_id: 12,
        position: PositionMm::new(-2_000, 0, 0),
        zone: Some(1),
        start_sample: 1_000,
        reference_spl_db_q8: 80 * 256,
        bands: BandEnergy::UNITY,
        directivity_q16: 0,
        forward_q15: [i16::MAX, 0, 0],
        class: if voice {
            SoundClass::Voice
        } else {
            SoundClass::Gunshot
        },
        priority: 10,
        voice: voice
            .then(|| EncodedVoice::new(VoiceStreamId(4), 8, &[1, 2, 3]))
            .transpose()?,
    })
}

#[test]
fn voice_is_gated_and_preserves_original_opus() -> Result<(), Error> {
    let mut world = fixture()?;
    world.set_receivers(&[receiver(false)])?;
    world.capture_emission(emission(true)?)?;
    let frame = world.step(1)?;
    assert_eq!(frame.voices.len(), 1);
    assert_eq!(frame.voices[0].encoded.payload(), &[1, 2, 3]);
    assert!(frame.voices[0].play_sample > 1_000);
    assert!(matches!(
        world.replay_facts().deliveries.as_slice(),
        [AcousticReplayDelivery::Voice { receiver_id: 2, .. }]
    ));
    assert_eq!(world.replay_facts().emissions[0].class, SoundClass::Voice);
    Ok(())
}

#[test]
fn bots_receive_no_source_identity_or_position() -> Result<(), Error> {
    let mut world = fixture()?;
    world.set_receivers(&[receiver(true)])?;
    world.capture_emission(emission(false)?)?;
    let frame = world.step(1)?;
    assert_eq!(frame.observations.len(), 1);
    assert_eq!(frame.observations[0].class, SoundClass::Gunshot);
    assert_ne!(frame.observations[0].observation_token, 99);
    Ok(())
}

#[test]
fn footsteps_gunshots_and_voice_share_path_and_arrival_math() -> Result<(), Error> {
    let mut world = fixture()?;
    world.set_receivers(&[receiver(false)])?;
    let mut footstep = emission(false)?;
    footstep.id = 1;
    footstep.class = SoundClass::Footstep;
    let mut gunshot = emission(false)?;
    gunshot.id = 2;
    gunshot.class = SoundClass::Gunshot;
    let mut voice = emission(true)?;
    voice.id = 3;
    world.capture_emission(footstep)?;
    world.capture_emission(gunshot)?;
    world.capture_emission(voice)?;
    let frame = world.step(1)?;
    assert_eq!(frame.sounds.len(), 2);
    assert_eq!(frame.voices.len(), 1);
    assert_eq!(frame.sounds[0].propagation, frame.sounds[1].propagation);
    assert_eq!(frame.sounds[0].propagation, frame.voices[0].propagation);
    assert_eq!(
        frame.sounds[0].play_sample,
        1_000 + arrival_delay_samples(frame.sounds[0].propagation.path_length_mm)
    );
    Ok(())
}

#[test]
fn committed_door_state_activates_on_next_tick() -> Result<(), Error> {
    let mut world = fixture()?;
    world.set_receivers(&[receiver(false)])?;
    world.stage_dynamic_state(AcousticDynamicState {
        instance_id: 10,
        state_id: Some(1),
        transform: QuantizedTransform::translated(PositionMm::default()),
        portal_open_q16: Some(0),
    })?;
    world.capture_emission(emission(false)?)?;
    let first_gain = world.step(1)?.sounds[0].propagation.band_gain;
    assert_eq!(world.structure_version(), AcousticStructureVersion(2));
    world.capture_emission(emission(false)?)?;
    let second_gain = world.step(2)?.sounds[0].propagation.band_gain;
    assert_ne!(first_gain, second_gain);
    Ok(())
}

#[test]
fn speed_of_sound_uses_integer_canonical_rounding() {
    assert_eq!(arrival_delay_samples(343_000), 48_000);
    assert_eq!(arrival_delay_samples(3_430), 480);
}

#[test]
fn destructible_variants_swap_geometry_without_rebake() -> Result<(), Error> {
    let mut world = fixture()?;
    world.set_receivers(&[receiver(false)])?;
    world.stage_dynamic_state(AcousticDynamicState {
        instance_id: 10,
        state_id: Some(1),
        transform: QuantizedTransform::translated(PositionMm::default()),
        portal_open_q16: Some(0),
    })?;
    world.step(1)?;

    world.stage_dynamic_state(AcousticDynamicState {
        instance_id: 10,
        state_id: Some(2),
        transform: QuantizedTransform::translated(PositionMm::default()),
        portal_open_q16: Some(0),
    })?;
    world.capture_emission(emission(false)?)?;
    let intact = world.step(2)?.sounds[0].propagation.band_gain;

    world.stage_dynamic_state(AcousticDynamicState {
        instance_id: 10,
        state_id: Some(3),
        transform: QuantizedTransform::translated(PositionMm::default()),
        portal_open_q16: Some(0),
    })?;
    world.capture_emission(emission(false)?)?;
    let breached = world.step(3)?.sounds[0].propagation.band_gain;

    world.capture_emission(emission(false)?)?;
    let removed = world.step(4)?.sounds[0].propagation.band_gain;
    assert_ne!(intact, breached);
    assert_ne!(breached, removed);
    assert_eq!(world.structure_version(), AcousticStructureVersion(4));
    Ok(())
}

#[test]
fn default_capacity_accepts_32_players_64_bots_and_limits_client_voice_frames() -> Result<(), Error>
{
    let settings = AcousticWorldSettings::default();
    assert_eq!(settings.max_receivers, 96);
    assert_eq!(settings.max_client_voices, 128);
    let mut world = fixture()?;
    let receivers = (0..96)
        .map(|id| AcousticReceiver {
            id,
            position: if id < 32 {
                PositionMm::new(2_000, 0, 0)
            } else {
                PositionMm::new(-2_000, 0, 0)
            },
            zone: Some(if id < 32 { 2 } else { 1 }),
            threshold_db_q8: 10 * 256,
            masking_db_q8: 0,
            hearing: BandEnergy::UNITY,
            bot: id >= 32,
        })
        .collect::<Vec<_>>();
    world.set_receivers(&receivers)?;
    world.capture_emission(emission(true)?)?;
    let frame = world.step(1)?;
    assert_eq!(frame.direct_pairs, 96);
    assert_eq!(frame.candidate_pairs, 32);
    assert_eq!(frame.voices.len(), 32);
    assert_eq!(frame.observations.len(), 64);

    world.set_receivers(&[receiver(false)])?;
    for sequence in 0..129_u32 {
        let mut voice = emission(true)?;
        voice.id = u64::from(sequence) + 1_000;
        voice.voice = Some(EncodedVoice::new(VoiceStreamId(4), sequence, &[1, 2, 3])?);
        world.capture_emission(voice)?;
    }
    assert_eq!(world.step(2)?.voices.len(), 128);
    Ok(())
}
