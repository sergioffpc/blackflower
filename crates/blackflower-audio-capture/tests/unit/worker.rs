use super::*;
use crate::{CaptureSettings, CaptureStream, VoiceActivation};

#[test]
fn mock_microphone_resamples_encodes_and_obeys_ptt() -> Result<(), Error> {
    let settings = CaptureSettings {
        activation: VoiceActivation::PushToTalk,
        ..CaptureSettings::default()
    };
    let mut stream = CaptureStream::mock(settings, 48_000, 2)?;
    stream.set_push_to_talk(true);
    let mut worker = stream.take_worker()?;
    let samples = (0..1_920)
        .map(|index| if index % 2 == 0 { 0.25 } else { -0.25 })
        .collect::<Vec<_>>();
    stream.push_mock_interleaved(&samples)?;
    let mut output = Vec::new();
    assert_eq!(worker.poll(&mut output)?, 1);
    assert!(!output[0].encoded.payload().is_empty());
    Ok(())
}

#[test]
fn energy_vad_drops_silence_and_keeps_the_sample_timeline() -> Result<(), Error> {
    let mut stream = CaptureStream::mock(CaptureSettings::default(), 48_000, 1)?;
    let mut worker = stream.take_worker()?;
    let mut output = Vec::new();
    stream.push_mock_interleaved(&[0.0; 960])?;
    assert_eq!(worker.poll(&mut output)?, 0);
    stream.push_mock_interleaved(&[0.25; 960])?;
    assert_eq!(worker.poll(&mut output)?, 1);
    assert_eq!(output[0].sample_timestamp, 960);
    assert!(output[0].energy.amplitude_q16 > 0);
    Ok(())
}
