use blackflower_audio_voice::{
    Application, Channels, Decoder, Encoder, Error, FrameDuration, OPUS_VERSION, SampleRate,
    version_string,
};

const FRAME_SAMPLES: usize = 960;

#[test]
fn bindings_report_the_pinned_opus_version() -> Result<(), Error> {
    assert_eq!(OPUS_VERSION, (1, 5, 2));
    assert_eq!(version_string()?, "libopus 1.5.2");
    assert_send::<Encoder>();
    assert_send::<Decoder>();
    Ok(())
}

#[test]
fn mono_voice_frame_round_trips_through_opus() -> Result<(), Error> {
    let mut encoder = Encoder::new(SampleRate::Hz48K, Channels::Mono, Application::Voip)?;
    encoder.set_bitrate(24_000)?;
    encoder.set_complexity(5)?;
    encoder.set_vbr(true)?;
    encoder.set_inband_fec(true)?;
    encoder.set_expected_packet_loss(10)?;
    encoder.set_dtx(false)?;

    let mut input = [0.0; FRAME_SAMPLES];
    for (index, sample) in input.iter_mut().enumerate() {
        *sample = if index % 8 < 4 { 0.2 } else { -0.2 };
    }
    let mut packet = [0; 1_500];
    let packet_length = encoder.encode(FrameDuration::Ms20, &input, &mut packet)?;
    assert!(packet_length > 0);

    let mut decoder = Decoder::new(SampleRate::Hz48K, Channels::Mono)?;
    let mut output = [0.0; FRAME_SAMPLES];
    let decoded = decoder.decode(&packet[..packet_length], &mut output)?;
    assert_eq!(decoded, FRAME_SAMPLES);
    assert!(output.iter().all(|sample| sample.is_finite()));
    assert!(output.iter().map(|sample| sample.abs()).sum::<f32>() > f32::EPSILON);
    Ok(())
}

#[test]
fn decoder_generates_packet_loss_concealment() -> Result<(), Error> {
    let mut decoder = Decoder::new(SampleRate::Hz48K, Channels::Mono)?;
    let mut output = [0.0; FRAME_SAMPLES];
    let decoded = decoder.conceal(FrameDuration::Ms20, &mut output)?;
    assert_eq!(decoded, FRAME_SAMPLES);
    assert!(output.iter().all(|sample| sample.is_finite()));
    Ok(())
}

#[test]
fn safe_api_rejects_invalid_lengths_and_configuration() -> Result<(), Error> {
    let mut encoder = Encoder::new(SampleRate::Hz48K, Channels::Mono, Application::Voip)?;
    let input = [0.0; FRAME_SAMPLES - 1];
    let mut packet = [0; 1_500];
    assert!(matches!(
        encoder.encode(FrameDuration::Ms20, &input, &mut packet),
        Err(Error::FrameLength {
            buffer: "encoder input",
            expected: FRAME_SAMPLES,
            actual
        }) if actual == FRAME_SAMPLES - 1
    ));
    assert!(matches!(
        encoder.set_complexity(11),
        Err(Error::ConfigurationOutOfRange {
            field: "complexity",
            minimum: 0,
            maximum: 10,
            actual: 11
        })
    ));
    assert!(matches!(
        encoder.set_bitrate(499),
        Err(Error::ConfigurationOutOfRange {
            field: "bitrate",
            minimum: 500,
            maximum: 512_000,
            actual: 499
        })
    ));

    let mut decoder = Decoder::new(SampleRate::Hz48K, Channels::Mono)?;
    let mut output = [0.0; FRAME_SAMPLES];
    assert!(matches!(
        decoder.decode(&[], &mut output),
        Err(Error::EmptyPacket)
    ));
    Ok(())
}

const fn assert_send<T: Send>() {}
