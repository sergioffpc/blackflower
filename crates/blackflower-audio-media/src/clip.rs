use std::sync::Arc;

use bytes::Bytes;

use crate::{AUDIO_CLIP_SCHEMA, AUDIO_SAMPLE_RATE, Error};

const MAGIC: &[u8; 8] = b"BFAUDIO\0";
const HEADER_LEN: usize = 44;
const NO_LOOP: u64 = u64::MAX;

/// Half-open frame range used for sample-accurate looping.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub struct LoopRegion {
    /// First frame in the loop.
    pub start: u64,
    /// First frame after the loop.
    pub end: u64,
}

impl LoopRegion {
    /// Validate and construct a loop range.
    pub fn new(start: u64, end: u64) -> Result<Self, Error> {
        if start >= end {
            return Err(Error::InvalidField("loop_region"));
        }
        Ok(Self { start, end })
    }
}

/// Fully decoded short-form audio clip.
#[derive(Debug, Clone, PartialEq, Eq)]
pub struct AudioClip {
    channels: u8,
    frame_count: u64,
    loop_region: Option<LoopRegion>,
    samples: Arc<[i16]>,
}

impl AudioClip {
    /// Decode and validate a `.bfaudio` object.
    #[allow(
        clippy::too_many_lines,
        reason = "the parser validates one compact fixed-header binary format in field order"
    )]
    pub fn from_bytes(bytes: Bytes) -> Result<Self, Error> {
        if bytes.len() < HEADER_LEN || &bytes[..8] != MAGIC {
            return Err(Error::InvalidContainer("missing .bfaudio header"));
        }
        let schema = read_u32(&bytes, 8)?;
        if schema != AUDIO_CLIP_SCHEMA {
            return Err(Error::InvalidContainer("unsupported .bfaudio schema"));
        }
        let sample_rate = read_u32(&bytes, 12)?;
        if sample_rate != AUDIO_SAMPLE_RATE {
            return Err(Error::InvalidContainer(".bfaudio is not 48 kHz"));
        }
        let channels = bytes[16];
        if !matches!(channels, 1 | 2) {
            return Err(Error::UnsupportedChannels(u32::from(channels)));
        }
        if bytes[17] != 0 || bytes[18] != 0 || bytes[19] != 0 {
            return Err(Error::InvalidContainer(
                "reserved .bfaudio bits are non-zero",
            ));
        }
        let frame_count = read_u64(&bytes, 20)?;
        if frame_count == 0 {
            return Err(Error::Empty);
        }
        let loop_start = read_u64(&bytes, 28)?;
        let loop_end = read_u64(&bytes, 36)?;
        let loop_region = match (loop_start, loop_end) {
            (NO_LOOP, NO_LOOP) => None,
            (start, end) => {
                let region = LoopRegion::new(start, end)?;
                if region.end > frame_count {
                    return Err(Error::InvalidField("loop_region"));
                }
                Some(region)
            }
        };
        let sample_count = usize::try_from(frame_count)
            .ok()
            .and_then(|frames| frames.checked_mul(usize::from(channels)))
            .ok_or(Error::InvalidContainer(".bfaudio sample count overflows"))?;
        let expected_len = HEADER_LEN
            .checked_add(
                sample_count
                    .checked_mul(2)
                    .ok_or(Error::InvalidContainer(".bfaudio byte count overflows"))?,
            )
            .ok_or(Error::InvalidContainer(".bfaudio byte count overflows"))?;
        if bytes.len() != expected_len {
            return Err(Error::InvalidContainer(".bfaudio payload length mismatch"));
        }
        let samples = bytes[HEADER_LEN..]
            .chunks_exact(2)
            .map(|sample| i16::from_le_bytes([sample[0], sample[1]]))
            .collect::<Vec<_>>();
        Ok(Self {
            channels,
            frame_count,
            loop_region,
            samples: samples.into(),
        })
    }

    /// Number of interleaved channels.
    #[must_use]
    pub const fn channels(&self) -> u8 {
        self.channels
    }

    /// Number of frames per channel.
    #[must_use]
    pub const fn frame_count(&self) -> u64 {
        self.frame_count
    }

    /// Optional sample-accurate loop range.
    #[must_use]
    pub const fn loop_region(&self) -> Option<LoopRegion> {
        self.loop_region
    }

    /// Interleaved signed PCM16 samples.
    #[must_use]
    pub fn samples(&self) -> &[i16] {
        &self.samples
    }

    pub(crate) fn encode(
        channels: u8,
        samples: &[i16],
        loop_region: Option<LoopRegion>,
    ) -> Result<Vec<u8>, Error> {
        if !matches!(channels, 1 | 2) {
            return Err(Error::UnsupportedChannels(u32::from(channels)));
        }
        if samples.is_empty() || !samples.len().is_multiple_of(usize::from(channels)) {
            return Err(Error::InvalidField("samples"));
        }
        let frame_count = u64::try_from(samples.len() / usize::from(channels))
            .map_err(|_error| Error::InvalidField("frame_count"))?;
        if loop_region.is_some_and(|region| region.end > frame_count) {
            return Err(Error::InvalidField("loop_region"));
        }
        let mut bytes = Vec::with_capacity(HEADER_LEN + samples.len() * 2);
        bytes.extend_from_slice(MAGIC);
        bytes.extend_from_slice(&AUDIO_CLIP_SCHEMA.to_le_bytes());
        bytes.extend_from_slice(&AUDIO_SAMPLE_RATE.to_le_bytes());
        bytes.push(channels);
        bytes.extend_from_slice(&[0; 3]);
        bytes.extend_from_slice(&frame_count.to_le_bytes());
        let (loop_start, loop_end) =
            loop_region.map_or((NO_LOOP, NO_LOOP), |region| (region.start, region.end));
        bytes.extend_from_slice(&loop_start.to_le_bytes());
        bytes.extend_from_slice(&loop_end.to_le_bytes());
        for sample in samples {
            bytes.extend_from_slice(&sample.to_le_bytes());
        }
        Ok(bytes)
    }
}

fn read_u32(bytes: &[u8], offset: usize) -> Result<u32, Error> {
    let value = bytes
        .get(offset..offset + 4)
        .ok_or(Error::InvalidContainer("truncated integer"))?;
    let array = <[u8; 4]>::try_from(value)
        .map_err(|_error| Error::InvalidContainer("truncated integer"))?;
    Ok(u32::from_le_bytes(array))
}

fn read_u64(bytes: &[u8], offset: usize) -> Result<u64, Error> {
    let value = bytes
        .get(offset..offset + 8)
        .ok_or(Error::InvalidContainer("truncated integer"))?;
    let array = <[u8; 8]>::try_from(value)
        .map_err(|_error| Error::InvalidContainer("truncated integer"))?;
    Ok(u64::from_le_bytes(array))
}

#[cfg(test)]
#[path = "../tests/unit/clip.rs"]
mod tests;
