use blackflower_acoustics::{AcousticStructureVersion, BandEnergy, EncodedVoice, VoiceStreamId};
use blackflower_audio_voice::{Application, Encoder};

use super::*;

fn propagation(play_sample: u64) -> PropagationDescriptor {
    PropagationDescriptor {
        structure_version: AcousticStructureVersion(1),
        arrival_sample: play_sample,
        path_length_mm: 3_430,
        gain_db_q8: -256,
        band_gain: BandEnergy::UNITY,
        direction_q15: [i16::MAX, 0, 0],
        uncertainty_q16: 0,
        direct: true,
    }
}

fn packet(
    encoder: &mut Encoder,
    sequence: u32,
    play_sample: u64,
) -> Result<AudibleVoiceDelivery, Error> {
    let mut bytes = [0_u8; blackflower_acoustics::MAX_OPUS_PACKET_BYTES];
    let pcm = [0.1_f32; 960];
    let len = encoder.encode(FrameDuration::Ms20, &pcm, &mut bytes)?;
    Ok(AudibleVoiceDelivery {
        receiver_id: 1,
        play_sample,
        propagation: propagation(play_sample),
        encoded: EncodedVoice::new(VoiceStreamId(9), sequence, &bytes[..len])
            .map_err(|_error| Error::InvalidField("test voice"))?,
    })
}

#[test]
fn jitter_buffer_schedules_packets_and_conceals_a_gap() -> Result<(), Error> {
    let mut encoder = Encoder::new(SampleRate::Hz48K, Channels::Mono, Application::Voip)?;
    let mut jitter = RemoteVoiceJitterBuffer::new(4)?;
    jitter.push(packet(&mut encoder, 2, 1_920)?)?;
    jitter.push(packet(&mut encoder, 0, 0)?)?;
    let mut pcm = [0.0_f32; 960];
    let first = jitter
        .decode_due(0, &mut pcm)?
        .ok_or(Error::InvalidField("missing first voice frame"))?;
    assert!(!first.concealed);
    let concealed = jitter
        .decode_due(1_920, &mut pcm)?
        .ok_or(Error::InvalidField("missing concealed voice frame"))?;
    assert!(concealed.concealed);
    let third = jitter
        .decode_due(1_920, &mut pcm)?
        .ok_or(Error::InvalidField("missing third voice frame"))?;
    assert!(!third.concealed);
    assert!(jitter.is_empty());
    Ok(())
}
