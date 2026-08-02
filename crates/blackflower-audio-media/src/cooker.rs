use std::io::Cursor;

use rubato::audioadapter_buffers::direct::SequentialSliceOfVecs;
use rubato::{
    Async, FixedAsync, Resampler, SincInterpolationParameters, SincInterpolationType,
    WindowFunction,
};

use crate::{AUDIO_SAMPLE_RATE, AudioClip, Error, LoopRegion};

/// Strict profile-derived lossless audio configuration.
#[derive(Debug, Clone, Copy, PartialEq, Eq, serde::Serialize, serde::Deserialize)]
#[serde(deny_unknown_fields)]
pub struct AudioCookSettings {
    /// Runtime/cooking sample rate. Must be 48,000.
    pub sample_rate: u32,
}

impl AudioCookSettings {
    /// Validate the versioned profile contract.
    pub fn validate(self) -> Result<(), Error> {
        if self.sample_rate != AUDIO_SAMPLE_RATE {
            return Err(Error::InvalidField("audio.sample_rate"));
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

/// Validate and preserve one authored 48 kHz FLAC stream losslessly.
pub fn cook_stream(
    extension: &str,
    source: &[u8],
    settings: AudioCookSettings,
) -> Result<Vec<u8>, Error> {
    settings.validate()?;
    if !extension.eq_ignore_ascii_case("flac") {
        return Err(Error::UnsupportedSource(
            "streaming audio source extension must be .flac",
        ));
    }
    let decoded = decode_flac(source)?;
    if decoded.sample_rate != settings.sample_rate {
        return Err(Error::InvalidField("audio stream sample_rate"));
    }
    Ok(source.to_vec())
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
        f_cutoff: Some(0.95),
        oversampling_factor: 160,
        interpolation: SincInterpolationType::Cubic,
        window: WindowFunction::BlackmanHarris2,
    };
    let channel_count = usize::from(source.channel_count);
    let input = SequentialSliceOfVecs::new(&source.channels, channel_count, input_frames)
        .map_err(|error| Error::Resample(error.to_string()))?;
    let mut resampler = Async::<f32>::new_sinc(
        ratio,
        1.0,
        &parameters,
        1_024,
        channel_count,
        FixedAsync::Input,
    )
    .map_err(|error| Error::Resample(error.to_string()))?;
    let interleaved = resampler
        .process_all(&input, input_frames, None)
        .map_err(|error| Error::Resample(error.to_string()))?;
    let mut output = deinterleave(
        AUDIO_SAMPLE_RATE,
        u16::from(source.channel_count),
        interleaved.take_data(),
    )?
    .channels;
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

#[cfg(test)]
#[path = "../tests/unit/cooker.rs"]
mod tests;
