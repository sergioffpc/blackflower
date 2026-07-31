use super::*;

#[test]
fn sound_event_round_trip_is_stable() -> Result<(), Error> {
    let event = SoundEvent {
        media: AssetId::from_str("audio/weapons/rifle")?,
        gain_db: -3.0,
        priority: 42,
        spatialization: Spatialization::Hrtf,
        loop_region: Some(LoopRegion::new(10, 20)?),
        attenuation: Some(Attenuation {
            min_distance: 1.0,
            max_distance: 50.0,
        }),
        concurrency: Some(Concurrency {
            group: "rifle".to_owned(),
            max_voices: 4,
        }),
    };
    let bytes = event.to_bytes()?;
    let decoded = SoundEvent::from_bytes(Bytes::from(bytes.clone()))?;
    assert_eq!(decoded, event);
    assert_eq!(decoded.to_bytes()?, bytes);
    Ok(())
}
