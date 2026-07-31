use std::io::Cursor;

use blackflower_audio_voice::{Channels, Decoder, SampleRate};
use bytes::Bytes;
use ogg::reading::PacketReader;

use crate::{AUDIO_SAMPLE_RATE, Error};

const OPUS_HEAD: &[u8; 8] = b"OpusHead";
const OPUS_TAGS: &[u8; 8] = b"OpusTags";
const MAX_OPUS_FRAMES: usize = 5_760;

/// One decoded stereo output frame.
#[derive(Debug, Clone, Copy, PartialEq, Default)]
pub struct AudioFrame {
    /// Left sample.
    pub left: f32,
    /// Right sample.
    pub right: f32,
}

/// Validated standard Ogg/Opus stream.
#[derive(Debug, Clone)]
pub struct AudioStream {
    bytes: Bytes,
    channels: Channels,
    pre_skip: u16,
    frame_count: usize,
}

impl AudioStream {
    /// Parse and validate one Ogg/Opus stream.
    pub fn from_bytes(bytes: Bytes) -> Result<Self, Error> {
        let parsed = parse_stream(&bytes)?;
        Ok(Self {
            bytes,
            channels: parsed.channels,
            pre_skip: parsed.pre_skip,
            frame_count: parsed.frame_count,
        })
    }

    /// Number of encoded channels.
    #[must_use]
    pub const fn channels(&self) -> u8 {
        match self.channels {
            Channels::Mono => 1,
            Channels::Stereo => 2,
        }
    }

    /// Number of audible frames after Opus pre-skip and end trim.
    #[must_use]
    pub const fn frame_count(&self) -> usize {
        self.frame_count
    }

    /// Construct an independent pull decoder.
    pub fn decoder(&self) -> Result<AudioStreamDecoder, Error> {
        AudioStreamDecoder::new(self)
    }
}

/// Pull-based Ogg/Opus decoder intended for a non-real-time worker thread.
#[derive(Debug)]
pub struct AudioStreamDecoder {
    channels: Channels,
    decoder: Decoder,
    packets: Vec<Vec<u8>>,
    packet_index: usize,
    initial_pre_skip: usize,
    pre_skip: usize,
    frame_count: usize,
    position: usize,
    pending: Vec<AudioFrame>,
}

impl AudioStreamDecoder {
    fn new(stream: &AudioStream) -> Result<Self, Error> {
        let parsed = parse_stream(&stream.bytes)?;
        Ok(Self {
            channels: parsed.channels,
            decoder: Decoder::new(SampleRate::Hz48K, parsed.channels)?,
            packets: parsed.packets,
            packet_index: 0,
            initial_pre_skip: usize::from(stream.pre_skip),
            pre_skip: usize::from(stream.pre_skip),
            frame_count: stream.frame_count,
            position: 0,
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

    /// Decode the next Opus packet.
    pub fn decode(&mut self) -> Result<Vec<AudioFrame>, Error> {
        if !self.pending.is_empty() {
            self.position = self
                .position
                .checked_add(self.pending.len())
                .ok_or(Error::InvalidField("stream position"))?;
            return Ok(std::mem::take(&mut self.pending));
        }
        if self.packet_index >= self.packets.len() || self.position >= self.frame_count {
            return Ok(Vec::new());
        }
        let packet = &self.packets[self.packet_index];
        self.packet_index += 1;
        let channels = self.channels.count();
        let mut samples = vec![0.0; MAX_OPUS_FRAMES * channels];
        let decoded = self.decoder.decode(packet, &mut samples)?;
        samples.truncate(decoded * channels);
        let skip = self.pre_skip.min(decoded);
        self.pre_skip -= skip;
        let remaining = decoded - skip;
        let allowed = remaining.min(self.frame_count - self.position);
        let first = skip * channels;
        let frames = samples[first..first + allowed * channels]
            .chunks_exact(channels)
            .map(|frame| match self.channels {
                Channels::Mono => AudioFrame {
                    left: frame[0],
                    right: frame[0],
                },
                Channels::Stereo => AudioFrame {
                    left: frame[0],
                    right: frame[1],
                },
            })
            .collect::<Vec<_>>();
        self.position += frames.len();
        Ok(frames)
    }

    /// Seek to an exact audible frame.
    pub fn seek(&mut self, target: usize) -> Result<usize, Error> {
        let target = target.min(self.frame_count);
        self.decoder.reset()?;
        self.packet_index = 0;
        self.pre_skip = self.initial_pre_skip;
        self.position = 0;
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

struct ParsedStream {
    channels: Channels,
    pre_skip: u16,
    frame_count: usize,
    packets: Vec<Vec<u8>>,
}

fn parse_stream(bytes: &[u8]) -> Result<ParsedStream, Error> {
    let mut reader = PacketReader::new(Cursor::new(bytes));
    let head = reader
        .read_packet()
        .map_err(|error| Error::Ogg(error.to_string()))?
        .ok_or(Error::InvalidContainer("Ogg/Opus stream is empty"))?;
    let (channels, pre_skip) = parse_opus_head(&head.data)?;
    let serial = head.stream_serial();
    let tags = reader
        .read_packet()
        .map_err(|error| Error::Ogg(error.to_string()))?
        .ok_or(Error::InvalidContainer("Ogg/Opus tags are missing"))?;
    if tags.stream_serial() != serial || !tags.data.starts_with(OPUS_TAGS) {
        return Err(Error::InvalidContainer("invalid Ogg/Opus tags"));
    }
    let mut packets = Vec::new();
    let mut final_granule = None;
    while let Some(packet) = reader
        .read_packet()
        .map_err(|error| Error::Ogg(error.to_string()))?
    {
        if packet.stream_serial() != serial {
            return Err(Error::InvalidContainer(
                "multiplexed Ogg streams are not supported",
            ));
        }
        if packet.data.is_empty() {
            return Err(Error::InvalidContainer("empty Opus packet"));
        }
        if packet.last_in_stream() {
            final_granule = Some(packet.absgp_page());
        }
        packets.push(packet.data);
    }
    let granule = final_granule.ok_or(Error::InvalidContainer("Ogg stream is not terminated"))?;
    let audible = granule
        .checked_sub(u64::from(pre_skip))
        .ok_or(Error::InvalidContainer("Opus granule precedes pre-skip"))?;
    let frame_count = usize::try_from(audible)
        .map_err(|_error| Error::InvalidContainer("Opus frame count overflows"))?;
    if packets.is_empty() || frame_count == 0 {
        return Err(Error::Empty);
    }
    Ok(ParsedStream {
        channels,
        pre_skip,
        frame_count,
        packets,
    })
}

fn parse_opus_head(bytes: &[u8]) -> Result<(Channels, u16), Error> {
    if bytes.len() != 19 || !bytes.starts_with(OPUS_HEAD) {
        return Err(Error::InvalidContainer("invalid OpusHead packet"));
    }
    if bytes[8] != 1 || bytes[18] != 0 {
        return Err(Error::InvalidContainer(
            "unsupported OpusHead version or mapping",
        ));
    }
    let channels = match bytes[9] {
        1 => Channels::Mono,
        2 => Channels::Stereo,
        value => return Err(Error::UnsupportedChannels(u32::from(value))),
    };
    let pre_skip = u16::from_le_bytes([bytes[10], bytes[11]]);
    let input_rate = u32::from_le_bytes([bytes[12], bytes[13], bytes[14], bytes[15]]);
    if input_rate != AUDIO_SAMPLE_RATE {
        return Err(Error::InvalidContainer("OpusHead input rate is not 48 kHz"));
    }
    if bytes[16] != 0 || bytes[17] != 0 {
        return Err(Error::InvalidContainer("Opus output gain must be zero"));
    }
    Ok((channels, pre_skip))
}
