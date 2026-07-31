use blackflower_acoustics::{
    AabbMm, AcousticBvh, AcousticMaterial, AcousticMaterialLibrary, AcousticReceiver,
    AcousticSimulationScene, AcousticTopology, AcousticWorld, AcousticWorldSettings,
    AcousticZoneVolume, BandEnergy, PositionMm, SoundClass, SoundEmission,
};
use blackflower_audio_capture::{
    CaptureSettings, CaptureStream, VoiceActivation, VoiceAnalyzerBank,
};
use blackflower_audio_voice::{Channels, Decoder, SampleRate};
use blackflower_networking::{
    HarnessEndpoint, InMemoryDatagramHarness, VoiceCapturePacket, decode_audible_voice,
    decode_voice_capture, encode_audible_voice, encode_voice_capture,
};

fn acoustic_world() -> Result<AcousticWorld, Box<dyn std::error::Error>> {
    let materials = AcousticMaterialLibrary::new(vec![AcousticMaterial {
        id: "concrete".to_owned(),
        absorption: BandEnergy([8_000, 12_000, 18_000]),
        scattering_q16: 4_000,
        transmission: BandEnergy([2_000, 500, 100]),
    }])?;
    let topology = AcousticTopology::new(
        vec![AcousticZoneVolume {
            id: 1,
            name: "room".to_owned(),
            bounds: AabbMm::new(
                PositionMm::new(-10_000, -10_000, -10_000),
                PositionMm::new(10_000, 10_000, 10_000),
            )?,
        }],
        Vec::new(),
        Vec::new(),
    )?;
    let scene = AcousticSimulationScene::new(AcousticSimulationScene {
        materials: "materials".to_owned(),
        topology: "topology".to_owned(),
        triangles: Vec::new(),
        bvh: AcousticBvh::build(&[])?,
        paths: Vec::new(),
        zones: Vec::new(),
    })?;
    Ok(AcousticWorld::new(
        materials,
        topology,
        scene,
        Vec::new(),
        AcousticWorldSettings::default(),
    )?)
}

#[test]
#[allow(
    clippy::too_many_lines,
    reason = "the vertical-slice acceptance test intentionally keeps every boundary in one proof"
)]
fn microphone_to_server_solver_bot_and_client_is_complete() -> Result<(), Box<dyn std::error::Error>>
{
    let settings = CaptureSettings {
        activation: VoiceActivation::PushToTalk,
        ..CaptureSettings::default()
    };
    let mut microphone = CaptureStream::mock(settings, 48_000, 1)?;
    microphone.set_push_to_talk(true);
    let mut capture_worker = microphone.take_worker()?;
    let input = (0..960)
        .map(|index| if index % 24 < 12 { 0.25 } else { -0.25 })
        .collect::<Vec<_>>();
    microphone.push_mock_interleaved(&input)?;
    let mut captured = Vec::new();
    assert_eq!(capture_worker.poll(&mut captured)?, 1);
    let captured = captured.remove(0);

    let mut link = InMemoryDatagramHarness::new(4);
    link.send(
        HarnessEndpoint::Server,
        encode_voice_capture(&VoiceCapturePacket {
            stream: captured.encoded.stream,
            sequence: captured.encoded.sequence,
            sample_timestamp: captured.sample_timestamp,
            encoded: captured.encoded.clone(),
        })?,
    )?;
    let server_datagram = link
        .receive(HarnessEndpoint::Server)
        .ok_or_else(|| std::io::Error::other("server packet is missing"))?;
    let packet = decode_voice_capture(&server_datagram)?;
    let mut analyzers = VoiceAnalyzerBank::new(32)?;
    let analyzed = analyzers.analyze(7, &packet.encoded)?;
    assert!(analyzed.amplitude_q16 > 0);

    let mut world = acoustic_world()?;
    world.set_receivers(&[
        AcousticReceiver {
            id: 10,
            position: PositionMm::new(1_000, 0, 0),
            zone: Some(1),
            threshold_db_q8: -100 * 256,
            masking_db_q8: -100 * 256,
            hearing: BandEnergy::UNITY,
            bot: true,
        },
        AcousticReceiver {
            id: 20,
            position: PositionMm::new(2_000, 0, 0),
            zone: Some(1),
            threshold_db_q8: -100 * 256,
            masking_db_q8: -100 * 256,
            hearing: BandEnergy::UNITY,
            bot: false,
        },
    ])?;
    world.capture_emission(SoundEmission {
        id: 700,
        client_event_id: 0,
        position: PositionMm::new(0, 0, 0),
        zone: Some(1),
        start_sample: packet.sample_timestamp,
        reference_spl_db_q8: 70 * 256,
        bands: analyzed
            .bands
            .multiplied(BandEnergy([analyzed.amplitude_q16; 3])),
        directivity_q16: 0,
        forward_q15: [0, 0, i16::MAX],
        class: SoundClass::Voice,
        priority: 128,
        voice: Some(packet.encoded.clone()),
    })?;
    let frame = world.step(0)?;
    assert_eq!(frame.observations.len(), 1);
    assert_eq!(frame.observations[0].class, SoundClass::Voice);
    assert_eq!(frame.voices.len(), 1);
    assert_eq!(frame.voices[0].encoded.payload(), packet.encoded.payload());
    assert!(frame.voices[0].propagation.arrival_sample > packet.sample_timestamp);

    link.send(
        HarnessEndpoint::Client,
        encode_audible_voice(&frame.voices[0])?,
    )?;
    let client_datagram = link
        .receive(HarnessEndpoint::Client)
        .ok_or_else(|| std::io::Error::other("client packet is missing"))?;
    let delivery = decode_audible_voice(&client_datagram, 20)?;
    assert_eq!(delivery.encoded.payload(), packet.encoded.payload());
    assert!(delivery.propagation.band_gain.peak() > 0);
    let mut client_decoder = Decoder::new(SampleRate::Hz48K, Channels::Mono)?;
    let mut pcm = [0.0_f32; 960];
    assert_eq!(
        client_decoder.decode(delivery.encoded.payload(), &mut pcm)?,
        960
    );
    Ok(())
}
