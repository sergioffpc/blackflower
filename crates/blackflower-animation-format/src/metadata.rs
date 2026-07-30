use crate::Error;

pub(crate) const METADATA_SCHEMA: u16 = 1;
const LOOPING_FLAG: u16 = 1;
const ADDITIVE_FLAG: u16 = 1 << 1;
const KNOWN_FLAGS: u16 = LOOPING_FLAG | ADDITIVE_FLAG;
const PREFIX_SIZE: usize = 16;

/// One named point on a normalized animation timeline.
#[derive(Debug, Clone, PartialEq)]
pub struct ClipMarker {
    name: String,
    ratio: f32,
}

impl ClipMarker {
    /// Construct a validated normalized marker.
    pub fn new(name: impl Into<String>, ratio: f32) -> Result<Self, Error> {
        let name = name.into();
        validate_name(&name)?;
        if !ratio.is_finite() || !(0.0..=1.0).contains(&ratio) {
            return Err(Error::InvalidMarkerRatio);
        }
        Ok(Self { name, ratio })
    }

    /// Return the marker name.
    #[must_use]
    pub fn name(&self) -> &str {
        &self.name
    }

    /// Return the normalized marker time.
    #[must_use]
    pub const fn ratio(&self) -> f32 {
        self.ratio
    }
}

/// Typed runtime metadata stored in one `.bfanim`.
#[derive(Debug, Clone, PartialEq)]
pub struct ClipMetadata {
    name: String,
    looping: bool,
    additive: bool,
    markers: Box<[ClipMarker]>,
}

impl ClipMetadata {
    /// Construct deterministically ordered clip metadata.
    pub fn new(
        name: impl Into<String>,
        looping: bool,
        additive: bool,
        markers: impl IntoIterator<Item = ClipMarker>,
    ) -> Result<Self, Error> {
        let name = name.into();
        validate_name(&name)?;
        let mut markers = markers.into_iter().collect::<Vec<_>>();
        markers.sort_by(|left, right| {
            left.ratio
                .total_cmp(&right.ratio)
                .then_with(|| left.name.cmp(&right.name))
        });
        validate_markers(&markers)?;
        Ok(Self {
            name,
            looping,
            additive,
            markers: markers.into_boxed_slice(),
        })
    }

    /// Return the clip name.
    #[must_use]
    pub fn name(&self) -> &str {
        &self.name
    }

    /// Whether playback policy loops the clip.
    #[must_use]
    pub const fn looping(&self) -> bool {
        self.looping
    }

    /// Whether the ozz payload contains additive transforms.
    #[must_use]
    pub const fn additive(&self) -> bool {
        self.additive
    }

    /// Return markers in deterministic timeline order.
    #[must_use]
    pub fn markers(&self) -> &[ClipMarker] {
        &self.markers
    }

    pub(crate) fn encode(&self) -> Result<Vec<u8>, Error> {
        let name_length = u32::try_from(self.name.len()).map_err(|_error| Error::AssetTooLarge)?;
        let marker_count =
            u32::try_from(self.markers.len()).map_err(|_error| Error::AssetTooLarge)?;
        let mut output = vec![0_u8; PREFIX_SIZE];
        write_u16(&mut output, 0, METADATA_SCHEMA)?;
        let flags =
            (u16::from(self.looping) * LOOPING_FLAG) | (u16::from(self.additive) * ADDITIVE_FLAG);
        write_u16(&mut output, 2, flags)?;
        write_u32(&mut output, 4, name_length)?;
        write_u32(&mut output, 8, marker_count)?;
        output.extend_from_slice(self.name.as_bytes());
        pad_to_four(&mut output);
        for marker in &self.markers {
            output.extend_from_slice(&marker.ratio.to_bits().to_le_bytes());
            let length = u32::try_from(marker.name.len()).map_err(|_error| Error::AssetTooLarge)?;
            output.extend_from_slice(&length.to_le_bytes());
            output.extend_from_slice(marker.name.as_bytes());
            pad_to_four(&mut output);
        }
        Ok(output)
    }

    pub(crate) fn decode(bytes: &[u8]) -> Result<Self, Error> {
        if bytes.len() < PREFIX_SIZE
            || read_u16(bytes, 0)? != METADATA_SCHEMA
            || read_u32(bytes, 12)? != 0
        {
            return Err(Error::InvalidClipMetadata);
        }
        let flags = read_u16(bytes, 2)?;
        if flags & !KNOWN_FLAGS != 0 {
            return Err(Error::InvalidClipMetadata);
        }
        let name_length = read_usize(bytes, 4)?;
        let marker_count = read_usize(bytes, 8)?;
        let mut cursor = PREFIX_SIZE;
        let name = read_text(bytes, &mut cursor, name_length)?;
        consume_padding(bytes, &mut cursor)?;
        let mut markers = Vec::with_capacity(marker_count);
        for _ in 0..marker_count {
            let ratio = f32::from_bits(read_u32_at_cursor(bytes, &mut cursor)?);
            let length = read_usize_at_cursor(bytes, &mut cursor)?;
            let marker_name = read_text(bytes, &mut cursor, length)?;
            consume_padding(bytes, &mut cursor)?;
            markers.push(ClipMarker::new(marker_name, ratio)?);
        }
        if cursor != bytes.len() {
            return Err(Error::InvalidClipMetadata);
        }
        validate_markers(&markers)?;
        Ok(Self {
            name,
            looping: flags & LOOPING_FLAG != 0,
            additive: flags & ADDITIVE_FLAG != 0,
            markers: markers.into_boxed_slice(),
        })
    }
}

fn validate_name(name: &str) -> Result<(), Error> {
    if name.is_empty() || name.trim() != name || name.chars().any(char::is_control) {
        Err(Error::InvalidName)
    } else {
        Ok(())
    }
}

fn validate_markers(markers: &[ClipMarker]) -> Result<(), Error> {
    for pair in markers.windows(2) {
        let ordering = pair[0]
            .ratio
            .total_cmp(&pair[1].ratio)
            .then_with(|| pair[0].name.cmp(&pair[1].name));
        if ordering.is_gt() {
            return Err(Error::InvalidMarkerOrder);
        }
        if pair[0].name == pair[1].name && pair[0].ratio.to_bits() == pair[1].ratio.to_bits() {
            return Err(Error::DuplicateMarker);
        }
    }
    Ok(())
}

fn pad_to_four(output: &mut Vec<u8>) {
    while !output.len().is_multiple_of(4) {
        output.push(0);
    }
}

fn consume_padding(bytes: &[u8], cursor: &mut usize) -> Result<(), Error> {
    let aligned = cursor
        .checked_add(3)
        .map(|value| value & !3)
        .ok_or(Error::InvalidClipMetadata)?;
    let padding = bytes
        .get(*cursor..aligned)
        .ok_or(Error::InvalidClipMetadata)?;
    if padding.iter().any(|byte| *byte != 0) {
        return Err(Error::InvalidClipMetadata);
    }
    *cursor = aligned;
    Ok(())
}

fn read_text(bytes: &[u8], cursor: &mut usize, length: usize) -> Result<String, Error> {
    let end = cursor
        .checked_add(length)
        .ok_or(Error::InvalidClipMetadata)?;
    let raw = bytes.get(*cursor..end).ok_or(Error::InvalidClipMetadata)?;
    let value = std::str::from_utf8(raw).map_err(|_error| Error::InvalidClipMetadata)?;
    validate_name(value)?;
    *cursor = end;
    Ok(value.to_owned())
}

fn read_u16(bytes: &[u8], offset: usize) -> Result<u16, Error> {
    let raw = bytes
        .get(offset..offset + 2)
        .ok_or(Error::InvalidClipMetadata)?;
    let array = <[u8; 2]>::try_from(raw).map_err(|_error| Error::InvalidClipMetadata)?;
    Ok(u16::from_le_bytes(array))
}

fn read_u32(bytes: &[u8], offset: usize) -> Result<u32, Error> {
    let raw = bytes
        .get(offset..offset + 4)
        .ok_or(Error::InvalidClipMetadata)?;
    let array = <[u8; 4]>::try_from(raw).map_err(|_error| Error::InvalidClipMetadata)?;
    Ok(u32::from_le_bytes(array))
}

fn read_usize(bytes: &[u8], offset: usize) -> Result<usize, Error> {
    usize::try_from(read_u32(bytes, offset)?).map_err(|_error| Error::InvalidClipMetadata)
}

fn read_u32_at_cursor(bytes: &[u8], cursor: &mut usize) -> Result<u32, Error> {
    let value = read_u32(bytes, *cursor)?;
    *cursor = cursor.checked_add(4).ok_or(Error::InvalidClipMetadata)?;
    Ok(value)
}

fn read_usize_at_cursor(bytes: &[u8], cursor: &mut usize) -> Result<usize, Error> {
    usize::try_from(read_u32_at_cursor(bytes, cursor)?).map_err(|_error| Error::InvalidClipMetadata)
}

fn write_u16(output: &mut [u8], offset: usize, value: u16) -> Result<(), Error> {
    let target = output
        .get_mut(offset..offset + 2)
        .ok_or(Error::AssetTooLarge)?;
    target.copy_from_slice(&value.to_le_bytes());
    Ok(())
}

fn write_u32(output: &mut [u8], offset: usize, value: u32) -> Result<(), Error> {
    let target = output
        .get_mut(offset..offset + 4)
        .ok_or(Error::AssetTooLarge)?;
    target.copy_from_slice(&value.to_le_bytes());
    Ok(())
}

#[cfg(test)]
mod tests {
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
}
