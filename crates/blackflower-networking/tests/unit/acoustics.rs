use super::*;

fn descriptor() -> PropagationDescriptor {
    PropagationDescriptor {
        structure_version: AcousticStructureVersion(4),
        arrival_sample: 9_000,
        path_length_mm: 3_430,
        gain_db_q8: -512,
        band_gain: BandEnergy([50_000, 40_000, 30_000]),
        direction_q15: [10, 20, 30],
        uncertainty_q16: 100,
        direct: false,
    }
}

#[test]
fn voice_round_trip_preserves_opus_exactly() -> Result<(), Box<dyn std::error::Error>> {
    let encoded = EncodedVoice::new(VoiceStreamId(3), 7, &[9, 8, 7, 6])?;
    let delivery = AudibleVoiceDelivery {
        receiver_id: 99,
        play_sample: 9_000,
        propagation: descriptor(),
        encoded,
    };
    let bytes = encode_audible_voice(&delivery)?;
    let decoded = decode_audible_voice(&bytes, 44)?;
    assert_eq!(decoded.receiver_id, 44);
    assert_eq!(decoded.encoded.payload(), &[9, 8, 7, 6]);
    Ok(())
}

#[test]
fn malformed_versions_sizes_duplicates_and_order_are_deterministic()
-> Result<(), Box<dyn std::error::Error>> {
    let encoded = EncodedVoice::new(VoiceStreamId(1), 1, &[1])?;
    let packet = VoiceCapturePacket {
        stream: VoiceStreamId(1),
        sequence: 1,
        sample_timestamp: 960,
        encoded,
    };
    let mut bytes = encode_voice_capture(&packet)?;
    bytes[4..6].copy_from_slice(&99_u16.to_le_bytes());
    assert_eq!(
        decode_voice_capture(&bytes),
        Err(DatagramError::Version(99))
    );
    assert_eq!(
        decode_voice_capture(&bytes[..6]),
        Err(DatagramError::Truncated)
    );

    let mut reorder = VoiceReorderBuffer::new(1);
    assert_eq!(
        reorder.push(packet.clone()),
        VoicePacketDisposition::Buffered
    );
    assert_eq!(reorder.push(packet), VoicePacketDisposition::Duplicate);
    assert_eq!(reorder.pop_ready().map(|value| value.sequence), Some(1));
    let too_far = VoiceCapturePacket {
        stream: VoiceStreamId(1),
        sequence: 5,
        sample_timestamp: 4_800,
        encoded: EncodedVoice::new(VoiceStreamId(1), 5, &[1])?,
    };
    assert_eq!(reorder.push(too_far), VoicePacketDisposition::Late);

    let mut trailing = encode_voice_capture(&VoiceCapturePacket {
        stream: VoiceStreamId(1),
        sequence: 2,
        sample_timestamp: 1_920,
        encoded: EncodedVoice::new(VoiceStreamId(1), 2, &[1])?,
    })?;
    trailing.push(0);
    assert_eq!(
        decode_voice_capture(&trailing),
        Err(DatagramError::Trailing)
    );
    assert_eq!(
        decode_voice_capture(&vec![0; MAX_ACOUSTIC_DATAGRAM_BYTES + 1]),
        Err(DatagramError::Oversized)
    );
    Ok(())
}
