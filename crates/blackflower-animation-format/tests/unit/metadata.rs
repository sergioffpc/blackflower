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
