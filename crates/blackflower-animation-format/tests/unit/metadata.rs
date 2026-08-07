use super::{ClipMarker, ClipMetadata};
use crate::Error;

#[test]
fn metadata_round_trips_and_orders_markers() -> Result<(), Error> {
    let metadata = ClipMetadata::new(
        "Walk",
        true,
        false,
        [
            ClipMarker::new("right", 0.75)?,
            ClipMarker::new("left", 0.25)?,
        ],
    )?;
    let decoded = ClipMetadata::decode(&metadata.encode()?)?;
    assert_eq!(decoded, metadata);
    assert_eq!(decoded.markers()[0].name(), "left");
    Ok(())
}

#[test]
fn duplicate_marker_is_rejected() -> Result<(), Error> {
    let result = ClipMetadata::new(
        "Walk",
        false,
        false,
        [
            ClipMarker::new("event", 0.5)?,
            ClipMarker::new("event", 0.5)?,
        ],
    );
    assert_eq!(result, Err(Error::DuplicateMarker));
    Ok(())
}

#[test]
fn marker_count_must_fit_the_remaining_metadata_bytes() {
    let mut bytes = vec![0_u8; 20];
    bytes[0..2].copy_from_slice(&super::METADATA_SCHEMA.to_le_bytes());
    bytes[4..8].copy_from_slice(&1_u32.to_le_bytes());
    bytes[8..12].copy_from_slice(&u32::MAX.to_le_bytes());
    bytes[16] = b'x';

    assert_eq!(
        ClipMetadata::decode(&bytes),
        Err(Error::InvalidClipMetadata)
    );
}
