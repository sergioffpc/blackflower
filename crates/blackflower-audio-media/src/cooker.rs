use std::io::Cursor;

use blackflower_audio_voice::{Application, Channels, Encoder, FrameDuration, SampleRate};
use ogg::writing::{PacketWriteEndInfo, PacketWriter};
use rubato::{
    Resampler, SincFixedIn, SincInterpolationParameters, SincInterpolationType, WindowFunction,
};

use crate::{AUDIO_SAMPLE_RATE, AudioClip, Error, LoopRegion};

const OPUS_FRAME_SAMPLES: usize = 960;
const OPUS_SERIAL: u32 = 0xBFA0_0001;
const OPUS_VENDOR: &[u8] = b"blackflower";

/// Strict profile-derived stream encoder configuration.
#[derive(Debug, Clone, Copy, PartialEq, Eq, serde::Serialize, serde::Deserialize)]
#[serde(deny_unknown_fields)]
pub struct AudioCookSettings {
    /// Runtime/cooking sample rate. Must be 48,000.
    pub sample_rate: u32,
    /// Opus frame duration. Must be 20 milliseconds.
    pub opus_frame_ms: u8,
    /// Opus encoder complexity from 0 through 10.
    pub opus_complexity: u8,
    /// Mono target bitrate in bits per second.
    pub opus_mono_bitrate: u32,
    /// Stereo target bitrate in bits per second.
    pub opus_stereo_bitrate: u32,
}

impl AudioCookSettings {
    /// Validate the versioned profile contract.
    pub fn validate(self) -> Result<(), Error> {
        if self.sample_rate != AUDIO_SAMPLE_RATE {
            return Err(Error::InvalidField("audio.sample_rate"));
        }
        if self.opus_frame_ms != 20 {
            return Err(Error::InvalidField("audio.opus_frame_ms"));
        }
        if self.opus_complexity > 10 {
            return Err(Error::InvalidField("audio.opus_complexity"));
        }
        if !(500..=512_000).contains(&self.opus_mono_bitrate)
            || !(500..=512_000).contains(&self.opus_stereo_bitrate)
        {
            return Err(Error::InvalidField("audio.opus_bitrate"));
        }
        Ok(())
    }
}

/// Cook authored WAV/FLAC into deterministic `.bfaudio`.
pub fn cook_clip(
    extension: &str,
    source: &[u8],
    loop_region: Option<LoopRegion>,
) -> Result<Vec<u8>, Error> {
    let decoded = decode_source(extension, source)?;
    let resampled = resample(decoded)?;
    let interleaved = interleave(&resampled.channels);
    let pcm = interleaved.into_iter().map(quantize).collect::<Vec<_>>();
    AudioClip::encode(resampled.channel_count, &pcm, loop_region)
}

/// Cook authored WAV/FLAC into deterministic standard Ogg/Opus.
pub fn cook_stream(
    extension: &str,
    source: &[u8],
    settings: AudioCookSettings,
) -> Result<Vec<u8>, Error> {
    settings.validate()?;
    let decoded = resample(decode_source(extension, source)?)?;
    encode_ogg_opus(&decoded, settings)
}

struct DecodedAudio {
    sample_rate: u32,
    channel_count: u8,
    channels: Vec<Vec<f32>>,
}

fn decode_source(extension: &str, source: &[u8]) -> Result<DecodedAudio, Error> {
    match extension.to_ascii_lowercase().as_str() {
        "wav" => decode_wav(source),
        "flac" => decode_flac(source),
        _ => Err(Error::UnsupportedSource(
            "audio source extension must be .wav or .flac",
        )),
    }
}

#[allow(
    clippy::cast_precision_loss,
    reason = "authored integer PCM is intentionally normalized into f32 mixer samples"
)]
fn decode_wav(source: &[u8]) -> Result<DecodedAudio, Error> {
    let mut reader = hound::WavReader::new(Cursor::new(source))?;
    let spec = reader.spec();
    validate_channels(u32::from(spec.channels))?;
    let samples = match spec.sample_format {
        hound::SampleFormat::Float => reader.samples::<f32>().collect::<Result<Vec<_>, _>>()?,
        hound::SampleFormat::Int => {
            let scale = integer_scale(u32::from(spec.bits_per_sample))?;
            reader
                .samples::<i32>()
                .map(|sample| sample.map(|value| value as f32 / scale))
                .collect::<Result<Vec<_>, _>>()?
        }
    };
    deinterleave(spec.sample_rate, spec.channels, samples)
}

#[allow(
    clippy::cast_precision_loss,
    reason = "authored integer PCM is intentionally normalized into f32 mixer samples"
)]
fn decode_flac(source: &[u8]) -> Result<DecodedAudio, Error> {
    let mut reader = claxon::FlacReader::new(Cursor::new(source))?;
    let info = reader.streaminfo();
    validate_channels(info.channels)?;
    let scale = integer_scale(info.bits_per_sample)?;
    let samples = reader
        .samples()
        .map(|sample| sample.map(|value| value as f32 / scale))
        .collect::<Result<Vec<_>, _>>()?;
    let channels =
        u16::try_from(info.channels).map_err(|_error| Error::InvalidField("channels"))?;
    deinterleave(info.sample_rate, channels, samples)
}

fn validate_channels(channels: u32) -> Result<(), Error> {
    if matches!(channels, 1 | 2) {
        Ok(())
    } else {
        Err(Error::UnsupportedChannels(channels))
    }
}

#[allow(
    clippy::cast_precision_loss,
    reason = "PCM integer normalization intentionally maps authored integers to f32 samples"
)]
fn integer_scale(bits: u32) -> Result<f32, Error> {
    if !(1..=32).contains(&bits) {
        return Err(Error::InvalidField("bits_per_sample"));
    }
    let scale = 1_u64
        .checked_shl(bits - 1)
        .ok_or(Error::InvalidField("bits_per_sample"))?;
    Ok(scale as f32)
}

fn deinterleave(sample_rate: u32, channels: u16, samples: Vec<f32>) -> Result<DecodedAudio, Error> {
    let channel_count = usize::from(channels);
    if samples.is_empty() || !samples.len().is_multiple_of(channel_count) {
        return Err(Error::Empty);
    }
    if samples.iter().any(|sample| !sample.is_finite()) {
        return Err(Error::InvalidField("sample"));
    }
    let mut planar = vec![Vec::with_capacity(samples.len() / channel_count); channel_count];
    for frame in samples.chunks_exact(channel_count) {
        for (channel, sample) in planar.iter_mut().zip(frame) {
            channel.push(*sample);
        }
    }
    Ok(DecodedAudio {
        sample_rate,
        channel_count: u8::try_from(channel_count)
            .map_err(|_error| Error::InvalidField("channels"))?,
        channels: planar,
    })
}

fn resample(source: DecodedAudio) -> Result<DecodedAudio, Error> {
    if source.sample_rate == AUDIO_SAMPLE_RATE {
        return Ok(source);
    }
    let input_frames = source.channels.first().map(Vec::len).ok_or(Error::Empty)?;
    let ratio = f64::from(AUDIO_SAMPLE_RATE) / f64::from(source.sample_rate);
    let parameters = SincInterpolationParameters {
        sinc_len: 256,
        f_cutoff: 0.95,
        oversampling_factor: 160,
        interpolation: SincInterpolationType::Cubic,
        window: WindowFunction::BlackmanHarris2,
    };
    let mut resampler = SincFixedIn::<f32>::new(
        ratio,
        1.0,
        parameters,
        input_frames,
        usize::from(source.channel_count),
    )
    .map_err(|error| Error::Resample(error.to_string()))?;
    let mut output = resampler
        .process(&source.channels, None)
        .map_err(|error| Error::Resample(error.to_string()))?;
    let tail = resampler
        .process_partial::<Vec<f32>>(None, None)
        .map_err(|error| Error::Resample(error.to_string()))?;
    for (channel, tail) in output.iter_mut().zip(tail) {
        channel.extend(tail);
    }
    let expected = target_frame_count(input_frames, source.sample_rate)?;
    for channel in &mut output {
        channel.resize(expected, 0.0);
        channel.truncate(expected);
    }
    Ok(DecodedAudio {
        sample_rate: AUDIO_SAMPLE_RATE,
        channel_count: source.channel_count,
        channels: output,
    })
}

fn target_frame_count(input_frames: usize, input_rate: u32) -> Result<usize, Error> {
    let input =
        u128::try_from(input_frames).map_err(|_error| Error::InvalidField("frame_count"))?;
    let numerator = input
        .checked_mul(u128::from(AUDIO_SAMPLE_RATE))
        .ok_or(Error::InvalidField("frame_count"))?;
    let rounded = numerator
        .checked_add(u128::from(input_rate / 2))
        .ok_or(Error::InvalidField("frame_count"))?
        / u128::from(input_rate);
    usize::try_from(rounded).map_err(|_error| Error::InvalidField("frame_count"))
}

fn interleave(channels: &[Vec<f32>]) -> Vec<f32> {
    let frames = channels.first().map_or(0, Vec::len);
    let mut output = Vec::with_capacity(frames * channels.len());
    for frame in 0..frames {
        for channel in channels {
            output.push(channel[frame]);
        }
    }
    output
}

#[allow(
    clippy::cast_possible_truncation,
    clippy::cast_precision_loss,
    reason = "PCM16 cooking deliberately rounds a clamped normalized f32 sample to i16"
)]
fn quantize(sample: f32) -> i16 {
    (sample.clamp(-1.0, 1.0) * f32::from(i16::MAX)).round() as i16
}

#[allow(
    clippy::too_many_lines,
    reason = "the encoder writes the three ordered Ogg Opus packet classes and their granules"
)]
fn encode_ogg_opus(audio: &DecodedAudio, settings: AudioCookSettings) -> Result<Vec<u8>, Error> {
    let channels = match audio.channel_count {
        1 => Channels::Mono,
        2 => Channels::Stereo,
        value => return Err(Error::UnsupportedChannels(u32::from(value))),
    };
    let mut encoder = Encoder::new(SampleRate::Hz48K, channels, Application::Audio)?;
    encoder.set_vbr(true)?;
    encoder.set_complexity(settings.opus_complexity)?;
    encoder.set_bitrate(match channels {
        Channels::Mono => settings.opus_mono_bitrate,
        Channels::Stereo => settings.opus_stereo_bitrate,
    })?;
    let pre_skip =
        u16::try_from(encoder.lookahead()?).map_err(|_error| Error::InvalidField("lookahead"))?;
    let frames = audio.channels[0].len();
    let interleaved = interleave(&audio.channels);
    let channel_count = channels.count();
    let packet_count = frames
        .checked_add(usize::from(pre_skip))
        .ok_or(Error::InvalidField("frame_count"))?
        .div_ceil(OPUS_FRAME_SAMPLES);
    let mut writer = PacketWriter::new(Vec::new());
    writer
        .write_packet(
            opus_head(audio.channel_count, pre_skip),
            OPUS_SERIAL,
            PacketWriteEndInfo::EndPage,
            0,
        )
        .map_err(|error| Error::Ogg(error.to_string()))?;
    writer
        .write_packet(opus_tags(), OPUS_SERIAL, PacketWriteEndInfo::EndPage, 0)
        .map_err(|error| Error::Ogg(error.to_string()))?;
    let mut input = vec![0.0; OPUS_FRAME_SAMPLES * channel_count];
    let mut packet = vec![0_u8; 4_000];
    for packet_index in 0..packet_count {
        input.fill(0.0);
        let start_frame = packet_index * OPUS_FRAME_SAMPLES;
        let end_frame = (start_frame + OPUS_FRAME_SAMPLES).min(frames);
        let source_start = start_frame * channel_count;
        let source_end = end_frame * channel_count;
        input[..source_end - source_start].copy_from_slice(&interleaved[source_start..source_end]);
        let length = encoder.encode(FrameDuration::Ms20, &input, &mut packet)?;
        let end = if packet_index + 1 == packet_count {
            PacketWriteEndInfo::EndStream
        } else {
            PacketWriteEndInfo::NormalPacket
        };
        let granule = if end == PacketWriteEndInfo::EndStream {
            u64::from(pre_skip)
                .checked_add(
                    u64::try_from(frames).map_err(|_error| Error::InvalidField("frame_count"))?,
                )
                .ok_or(Error::InvalidField("granule_position"))?
        } else {
            u64::try_from((packet_index + 1) * OPUS_FRAME_SAMPLES)
                .map_err(|_error| Error::InvalidField("granule_position"))?
        };
        writer
            .write_packet(packet[..length].to_vec(), OPUS_SERIAL, end, granule)
            .map_err(|error| Error::Ogg(error.to_string()))?;
    }
    Ok(writer.into_inner())
}

fn opus_head(channels: u8, pre_skip: u16) -> Vec<u8> {
    let mut packet = Vec::with_capacity(19);
    packet.extend_from_slice(b"OpusHead");
    packet.push(1);
    packet.push(channels);
    packet.extend_from_slice(&pre_skip.to_le_bytes());
    packet.extend_from_slice(&AUDIO_SAMPLE_RATE.to_le_bytes());
    packet.extend_from_slice(&0_i16.to_le_bytes());
    packet.push(0);
    packet
}

fn opus_tags() -> Vec<u8> {
    let mut packet = Vec::new();
    packet.extend_from_slice(b"OpusTags");
    packet.extend_from_slice(&u32::try_from(OPUS_VENDOR.len()).unwrap_or(0).to_le_bytes());
    packet.extend_from_slice(OPUS_VENDOR);
    packet.extend_from_slice(&0_u32.to_le_bytes());
    packet
}

#[cfg(test)]
mod tests {
    use bytes::Bytes;

    use super::*;
    use crate::{AudioClip, AudioStream};

    #[test]
    fn clip_cook_is_deterministic_and_resamples_to_48khz() -> Result<(), Error> {
        let source = pcm16_wav(24_000, 1, 240);
        let first = cook_clip("wav", &source, None)?;
        let second = cook_clip("wav", &source, None)?;
        assert_eq!(first, second);
        let clip = AudioClip::from_bytes(Bytes::from(first))?;
        assert_eq!(clip.frame_count(), 480);
        assert_eq!(clip.channels(), 1);
        Ok(())
    }

    #[test]
    fn stream_cook_round_trips_frame_count_and_seek() -> Result<(), Error> {
        let source = pcm16_wav(48_000, 2, 1_920);
        let settings = AudioCookSettings {
            sample_rate: 48_000,
            opus_frame_ms: 20,
            opus_complexity: 10,
            opus_mono_bitrate: 64_000,
            opus_stereo_bitrate: 128_000,
        };
        let bytes = cook_stream("wav", &source, settings)?;
        let stream = AudioStream::from_bytes(Bytes::from(bytes))?;
        assert_eq!(stream.frame_count(), 1_920);
        assert_eq!(stream.channels(), 2);
        let mut decoder = stream.decoder()?;
        assert_eq!(decoder.seek(1_000)?, 1_000);
        let frames = decoder.decode()?;
        assert!(!frames.is_empty());
        assert_eq!(decoder.position(), 1_920.min(1_000 + frames.len()));
        Ok(())
    }

    fn pcm16_wav(sample_rate: u32, channels: u16, frames: u32) -> Vec<u8> {
        let sample_count = frames * u32::from(channels);
        let data_len = sample_count * 2;
        let mut bytes = Vec::new();
        bytes.extend_from_slice(b"RIFF");
        bytes.extend_from_slice(&(36 + data_len).to_le_bytes());
        bytes.extend_from_slice(b"WAVEfmt ");
        bytes.extend_from_slice(&16_u32.to_le_bytes());
        bytes.extend_from_slice(&1_u16.to_le_bytes());
        bytes.extend_from_slice(&channels.to_le_bytes());
        bytes.extend_from_slice(&sample_rate.to_le_bytes());
        bytes.extend_from_slice(&(sample_rate * u32::from(channels) * 2).to_le_bytes());
        bytes.extend_from_slice(&(channels * 2).to_le_bytes());
        bytes.extend_from_slice(&16_u16.to_le_bytes());
        bytes.extend_from_slice(b"data");
        bytes.extend_from_slice(&data_len.to_le_bytes());
        for index in 0..sample_count {
            let sample = i16::try_from(index % 2_048).unwrap_or(0) - 1_024;
            bytes.extend_from_slice(&sample.to_le_bytes());
        }
        bytes
    }
}
