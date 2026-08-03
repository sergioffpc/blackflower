use super::*;

#[test]
fn clip_round_trip_preserves_frames_and_loop() -> Result<(), Error> {
    let region = LoopRegion::new(1, 3)?;
    let bytes = AudioClip::encode(1, &[1, -2, 3], Some(region))?;
    let clip = AudioClip::from_bytes(Bytes::from(bytes))?;
    assert_eq!(clip.channels(), 1);
    assert_eq!(clip.frame_count(), 3);
    assert_eq!(clip.loop_region(), Some(region));
    assert_eq!(clip.samples(), &[1, -2, 3]);
    Ok(())
}

#[test]
fn clip_rejects_truncated_payload() -> Result<(), Error> {
    let mut bytes = AudioClip::encode(1, &[1], None)?;
    let _last = bytes.pop();
    assert!(AudioClip::from_bytes(Bytes::from(bytes)).is_err());
    Ok(())
}
