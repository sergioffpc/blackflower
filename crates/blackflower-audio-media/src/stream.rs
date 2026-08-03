use std::io::Cursor;

use bytes::Bytes;
use claxon::{Block, FlacReader};

use crate::{AUDIO_SAMPLE_RATE, Error};

/// One decoded stereo output frame.
#[derive(Debug, Clone, Copy, PartialEq, Default)]
pub struct AudioFrame {
    /// Left sample.
    pub left: f32,
    /// Right sample.
    pub right: f32,
}

/// Validated native FLAC stream.
#[derive(Debug, Clone)]
pub struct AudioStream {
    bytes: Bytes,
    channels: u8,
    bits_per_sample: u32,
    frame_count: usize,
    max_block_size: usize,
}

impl AudioStream {
    /// Parse and validate one mono or stereo 48 kHz FLAC stream.
    pub fn from_bytes(bytes: Bytes) -> Result<Self, Error> {
        let metadata = stream_metadata(bytes.clone())?;
        Ok(Self {
            bytes,
            channels: metadata.channels,
            bits_per_sample: metadata.bits_per_sample,
            frame_count: metadata.frame_count,
            max_block_size: metadata.max_block_size,
        })
    }

    /// Number of encoded channels.
    #[must_use]
    pub const fn channels(&self) -> u8 {
        self.channels
    }

    /// Number of lossless PCM frames declared by FLAC STREAMINFO.
    #[must_use]
    pub const fn frame_count(&self) -> usize {
        self.frame_count
    }

    /// Construct an independent pull decoder.
    pub fn decoder(&self) -> Result<AudioStreamDecoder, Error> {
        AudioStreamDecoder::new(self)
    }
}

/// Pull-based FLAC decoder intended for a non-real-time worker thread.
pub struct AudioStreamDecoder {
    bytes: Bytes,
    reader: FlacReader<Cursor<Bytes>>,
    channels: u8,
    bits_per_sample: u32,
    frame_count: usize,
    position: usize,
    decoded_position: usize,
    block_buffer: Vec<i32>,
    pending: Vec<AudioFrame>,
}

impl AudioStreamDecoder {
    fn new(stream: &AudioStream) -> Result<Self, Error> {
        let capacity = stream
            .max_block_size
            .checked_mul(usize::from(stream.channels))
            .ok_or(Error::InvalidField("FLAC block size"))?;
        Ok(Self {
            bytes: stream.bytes.clone(),
            reader: open_reader(stream.bytes.clone())?,
            channels: stream.channels,
            bits_per_sample: stream.bits_per_sample,
            frame_count: stream.frame_count,
            position: 0,
            decoded_position: 0,
            block_buffer: Vec::with_capacity(capacity),
            pending: Vec::new(),
        })
    }

    /// Output sample rate.
    #[must_use]
    pub const fn sample_rate(&self) -> u32 {
        AUDIO_SAMPLE_RATE
    }

    /// Total number of audible frames.
    #[must_use]
    pub const fn frame_count(&self) -> usize {
        self.frame_count
    }

    /// Current audible frame position.
    #[must_use]
    pub const fn position(&self) -> usize {
        self.position
    }

    /// Decode the next lossless FLAC block.
    pub fn decode(&mut self) -> Result<Vec<AudioFrame>, Error> {
        if !self.pending.is_empty() {
            let frames = std::mem::take(&mut self.pending);
            self.position = self
                .position
                .checked_add(frames.len())
                .ok_or(Error::InvalidField("stream position"))?;
            return Ok(frames);
        }
        if self.position >= self.frame_count {
            return Ok(Vec::new());
        }
        let block = self
            .reader
            .blocks()
            .read_next_or_eof(std::mem::take(&mut self.block_buffer))?;
        let Some(block) = block else {
            return Err(Error::InvalidContainer(
                "FLAC ended before its declared frame count",
            ));
        };
        if block.channels() != u32::from(self.channels) {
            return Err(Error::InvalidContainer("FLAC channel count changed"));
        }
        let block_start = usize::try_from(block.time())
            .map_err(|_error| Error::InvalidContainer("FLAC block position overflows"))?;
        if block_start != self.decoded_position {
            return Err(Error::InvalidContainer("FLAC blocks are not contiguous"));
        }
        let duration = usize::try_from(block.duration())
            .map_err(|_error| Error::InvalidContainer("FLAC block size overflows"))?;
        let available = self.frame_count.saturating_sub(self.position);
        let emitted = duration.min(available);
        let frames = block_frames(&block, self.channels, self.bits_per_sample, emitted)?;
        self.decoded_position = block_start
            .checked_add(duration)
            .ok_or(Error::InvalidField("stream position"))?;
        self.block_buffer = block.into_buffer();
        self.position = self
            .position
            .checked_add(frames.len())
            .ok_or(Error::InvalidField("stream position"))?;
        Ok(frames)
    }

    /// Seek to an exact audible frame by restarting the worker decoder.
    pub fn seek(&mut self, target: usize) -> Result<usize, Error> {
        let target = target.min(self.frame_count);
        self.reader = open_reader(self.bytes.clone())?;
        self.position = 0;
        self.decoded_position = 0;
        self.pending.clear();
        while self.position < target {
            let before = self.position;
            let frames = self.decode()?;
            if frames.is_empty() {
                break;
            }
            if self.position > target {
                let consumed = target - before;
                self.pending.extend_from_slice(&frames[consumed..]);
                self.position = target;
            }
        }
        Ok(self.position)
    }
}

struct StreamMetadata {
    channels: u8,
    bits_per_sample: u32,
    frame_count: usize,
    max_block_size: usize,
}

fn open_reader(bytes: Bytes) -> Result<FlacReader<Cursor<Bytes>>, Error> {
    FlacReader::new(Cursor::new(bytes)).map_err(Error::from)
}

fn stream_metadata(bytes: Bytes) -> Result<StreamMetadata, Error> {
    let reader = open_reader(bytes)?;
    let info = reader.streaminfo();
    if info.sample_rate != AUDIO_SAMPLE_RATE {
        return Err(Error::InvalidContainer("FLAC sample rate is not 48 kHz"));
    }
    if !matches!(info.channels, 1 | 2) {
        return Err(Error::UnsupportedChannels(info.channels));
    }
    let channels = u8::try_from(info.channels)
        .map_err(|_error| Error::InvalidContainer("FLAC channel count overflows"))?;
    let frame_count = info
        .samples
        .ok_or(Error::InvalidContainer("FLAC frame count is unknown"))
        .and_then(|frames| {
            usize::try_from(frames)
                .map_err(|_error| Error::InvalidContainer("FLAC frame count overflows"))
        })?;
    if frame_count == 0 || info.max_block_size == 0 {
        return Err(Error::Empty);
    }
    let _scale = pcm_scale(info.bits_per_sample)?;
    Ok(StreamMetadata {
        channels,
        bits_per_sample: info.bits_per_sample,
        frame_count,
        max_block_size: usize::from(info.max_block_size),
    })
}

#[allow(
    clippy::cast_precision_loss,
    reason = "lossless integer PCM is intentionally normalized into f32 mixer samples"
)]
fn block_frames(
    block: &Block,
    channels: u8,
    bits_per_sample: u32,
    emitted: usize,
) -> Result<Vec<AudioFrame>, Error> {
    let scale = pcm_scale(bits_per_sample)?;
    let mut frames = Vec::with_capacity(emitted);
    match channels {
        1 => {
            frames.extend(block.channel(0).iter().take(emitted).map(|sample| {
                let value = *sample as f32 / scale;
                AudioFrame {
                    left: value,
                    right: value,
                }
            }));
        }
        2 => {
            frames.extend(
                block
                    .channel(0)
                    .iter()
                    .zip(block.channel(1))
                    .take(emitted)
                    .map(|(left, right)| AudioFrame {
                        left: *left as f32 / scale,
                        right: *right as f32 / scale,
                    }),
            );
        }
        _ => return Err(Error::UnsupportedChannels(u32::from(channels))),
    }
    Ok(frames)
}

#[allow(
    clippy::cast_precision_loss,
    reason = "lossless integer PCM is intentionally normalized into f32 mixer samples"
)]
fn pcm_scale(bits_per_sample: u32) -> Result<f32, Error> {
    if !(1..=32).contains(&bits_per_sample) {
        return Err(Error::InvalidContainer("invalid FLAC bits per sample"));
    }
    let scale = 1_u64
        .checked_shl(bits_per_sample - 1)
        .ok_or(Error::InvalidContainer("invalid FLAC bits per sample"))?;
    Ok(scale as f32)
}
