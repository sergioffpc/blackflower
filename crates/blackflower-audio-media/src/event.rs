use std::str::FromStr;

use blackflower_assets::AssetId;
use bytes::Bytes;

use crate::{Error, LoopRegion, SOUND_EVENT_SCHEMA};

const MAGIC: &[u8; 8] = b"BFSOUND\0";

/// Spatial rendering selected by a sound event.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum Spatialization {
    /// Conventional stereo playback.
    TwoDimensional,
    /// Steam Audio binaural HRTF playback.
    Hrtf,
}

/// Distance attenuation policy in metres.
#[derive(Debug, Clone, Copy, PartialEq)]
pub struct Attenuation {
    /// Distance at which attenuation begins.
    pub min_distance: f32,
    /// Distance at which the voice reaches silence.
    pub max_distance: f32,
}

/// Per-group simultaneous voice limit.
#[derive(Debug, Clone, PartialEq, Eq)]
pub struct Concurrency {
    /// Stable policy group.
    pub group: String,
    /// Maximum number of live voices in the group.
    pub max_voices: u16,
}

/// Source-less playback policy referencing one cooked audio media asset.
#[derive(Debug, Clone, PartialEq)]
pub struct SoundEvent {
    /// Referenced `audio_clip` or `audio_stream` asset.
    pub media: AssetId,
    /// Event gain in decibels.
    pub gain_db: f32,
    /// Voice-stealing priority. Higher values win.
    pub priority: u8,
    /// Spatial rendering policy.
    pub spatialization: Spatialization,
    /// Optional sample-accurate loop override.
    pub loop_region: Option<LoopRegion>,
    /// Optional distance attenuation.
    pub attenuation: Option<Attenuation>,
    /// Optional concurrency group.
    pub concurrency: Option<Concurrency>,
}

impl SoundEvent {
    /// Validate this authored event.
    pub fn validate(&self) -> Result<(), Error> {
        if !self.gain_db.is_finite() {
            return Err(Error::InvalidField("gain_db"));
        }
        if let Some(attenuation) = self.attenuation
            && (!attenuation.min_distance.is_finite()
                || !attenuation.max_distance.is_finite()
                || attenuation.min_distance < 0.0
                || attenuation.min_distance >= attenuation.max_distance)
        {
            return Err(Error::InvalidField("attenuation"));
        }
        if let Some(concurrency) = &self.concurrency
            && (concurrency.group.is_empty()
                || concurrency.group.len() > usize::from(u16::MAX)
                || concurrency.max_voices == 0)
        {
            return Err(Error::InvalidField("concurrency"));
        }
        Ok(())
    }

    /// Encode a deterministic `.bfsound` object.
    pub fn to_bytes(&self) -> Result<Vec<u8>, Error> {
        self.validate()?;
        let media = self.media.as_str().as_bytes();
        let media_len =
            u16::try_from(media.len()).map_err(|_error| Error::InvalidField("media"))?;
        let mut bytes = Vec::new();
        bytes.extend_from_slice(MAGIC);
        bytes.extend_from_slice(&SOUND_EVENT_SCHEMA.to_le_bytes());
        bytes.extend_from_slice(&media_len.to_le_bytes());
        bytes.extend_from_slice(media);
        bytes.extend_from_slice(&self.gain_db.to_bits().to_le_bytes());
        bytes.push(self.priority);
        bytes.push(match self.spatialization {
            Spatialization::TwoDimensional => 0,
            Spatialization::Hrtf => 1,
        });
        encode_loop(&mut bytes, self.loop_region);
        match self.attenuation {
            None => bytes.push(0),
            Some(value) => {
                bytes.push(1);
                bytes.extend_from_slice(&value.min_distance.to_bits().to_le_bytes());
                bytes.extend_from_slice(&value.max_distance.to_bits().to_le_bytes());
            }
        }
        match &self.concurrency {
            None => bytes.push(0),
            Some(value) => {
                bytes.push(1);
                bytes.extend_from_slice(&value.max_voices.to_le_bytes());
                let group = value.group.as_bytes();
                let length = u16::try_from(group.len())
                    .map_err(|_error| Error::InvalidField("concurrency.group"))?;
                bytes.extend_from_slice(&length.to_le_bytes());
                bytes.extend_from_slice(group);
            }
        }
        Ok(bytes)
    }

    /// Decode and validate a `.bfsound` object.
    pub fn from_bytes(bytes: Bytes) -> Result<Self, Error> {
        let mut reader = Reader::new(&bytes);
        if reader.take(8)? != MAGIC {
            return Err(Error::InvalidContainer("missing .bfsound header"));
        }
        if reader.u32()? != SOUND_EVENT_SCHEMA {
            return Err(Error::InvalidContainer("unsupported .bfsound schema"));
        }
        let media_len = usize::from(reader.u16()?);
        let media = std::str::from_utf8(reader.take(media_len)?)
            .map_err(|_error| Error::InvalidContainer("media ID is not UTF-8"))?;
        let event = Self {
            media: AssetId::from_str(media)?,
            gain_db: f32::from_bits(reader.u32()?),
            priority: reader.u8()?,
            spatialization: match reader.u8()? {
                0 => Spatialization::TwoDimensional,
                1 => Spatialization::Hrtf,
                _ => return Err(Error::InvalidContainer("unknown spatialization policy")),
            },
            loop_region: decode_loop(&mut reader)?,
            attenuation: match reader.u8()? {
                0 => None,
                1 => Some(Attenuation {
                    min_distance: f32::from_bits(reader.u32()?),
                    max_distance: f32::from_bits(reader.u32()?),
                }),
                _ => return Err(Error::InvalidContainer("unknown attenuation flag")),
            },
            concurrency: match reader.u8()? {
                0 => None,
                1 => {
                    let max_voices = reader.u16()?;
                    let length = usize::from(reader.u16()?);
                    let group = std::str::from_utf8(reader.take(length)?)
                        .map_err(|_error| {
                            Error::InvalidContainer("concurrency group is not UTF-8")
                        })?
                        .to_owned();
                    Some(Concurrency { group, max_voices })
                }
                _ => return Err(Error::InvalidContainer("unknown concurrency flag")),
            },
        };
        if !reader.is_empty() {
            return Err(Error::InvalidContainer("trailing .bfsound bytes"));
        }
        event.validate()?;
        Ok(event)
    }
}

fn encode_loop(bytes: &mut Vec<u8>, region: Option<LoopRegion>) {
    match region {
        None => bytes.push(0),
        Some(value) => {
            bytes.push(1);
            bytes.extend_from_slice(&value.start.to_le_bytes());
            bytes.extend_from_slice(&value.end.to_le_bytes());
        }
    }
}

fn decode_loop(reader: &mut Reader<'_>) -> Result<Option<LoopRegion>, Error> {
    match reader.u8()? {
        0 => Ok(None),
        1 => Ok(Some(LoopRegion::new(reader.u64()?, reader.u64()?)?)),
        _ => Err(Error::InvalidContainer("unknown loop flag")),
    }
}

struct Reader<'a> {
    bytes: &'a [u8],
    offset: usize,
}

impl<'a> Reader<'a> {
    const fn new(bytes: &'a [u8]) -> Self {
        Self { bytes, offset: 0 }
    }

    fn take(&mut self, length: usize) -> Result<&'a [u8], Error> {
        let end = self
            .offset
            .checked_add(length)
            .ok_or(Error::InvalidContainer("field length overflows"))?;
        let value = self
            .bytes
            .get(self.offset..end)
            .ok_or(Error::InvalidContainer("truncated .bfsound"))?;
        self.offset = end;
        Ok(value)
    }

    fn u8(&mut self) -> Result<u8, Error> {
        self.take(1).map(|bytes| bytes[0])
    }

    fn u16(&mut self) -> Result<u16, Error> {
        let bytes = <[u8; 2]>::try_from(self.take(2)?)
            .map_err(|_error| Error::InvalidContainer("truncated integer"))?;
        Ok(u16::from_le_bytes(bytes))
    }

    fn u32(&mut self) -> Result<u32, Error> {
        let bytes = <[u8; 4]>::try_from(self.take(4)?)
            .map_err(|_error| Error::InvalidContainer("truncated integer"))?;
        Ok(u32::from_le_bytes(bytes))
    }

    fn u64(&mut self) -> Result<u64, Error> {
        let bytes = <[u8; 8]>::try_from(self.take(8)?)
            .map_err(|_error| Error::InvalidContainer("truncated integer"))?;
        Ok(u64::from_le_bytes(bytes))
    }

    fn is_empty(&self) -> bool {
        self.offset == self.bytes.len()
    }
}

#[cfg(test)]
mod tests {
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
}
