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
fn clip_cook_resamples_fractional_stereo_rate_to_exact_frame_count() -> Result<(), Error> {
    let source = pcm16_wav(44_100, 2, 442);
    let clip = AudioClip::from_bytes(Bytes::from(cook_clip("wav", &source, None)?))?;
    assert_eq!(clip.frame_count(), 481);
    assert_eq!(clip.channels(), 2);
    Ok(())
}

#[test]
fn stream_cook_round_trips_frame_count_and_seek() -> Result<(), Error> {
    let source = flac_48k_stereo_1_920()?;
    let settings = AudioCookSettings {
        sample_rate: 48_000,
    };
    let bytes = cook_stream("flac", &source, settings)?;
    assert_eq!(bytes, source);
    let stream = AudioStream::from_bytes(Bytes::from(bytes))?;
    assert_eq!(stream.frame_count(), 1_920);
    assert_eq!(stream.channels(), 2);
    let mut decoder = stream.decoder()?;
    assert_eq!(decoder.seek(1_000)?, 1_000);
    let frames = decoder.decode()?;
    assert!(!frames.is_empty());
    assert_eq!(decoder.position(), 1_920.min(1_000 + frames.len()));
    assert!(matches!(
        cook_stream("wav", &pcm16_wav(48_000, 2, 1_920), settings),
        Err(Error::UnsupportedSource(_))
    ));
    let mut corrupted = source;
    let Some(checksum) = corrupted.last_mut() else {
        return Err(Error::InvalidField("test FLAC"));
    };
    *checksum ^= 1;
    let corrupted = AudioStream::from_bytes(Bytes::from(corrupted))?;
    assert!(matches!(corrupted.decoder()?.decode(), Err(Error::Flac(_))));
    Ok(())
}

fn flac_48k_stereo_1_920() -> Result<Vec<u8>, Error> {
    const HEX: &str = "664c614300000022100010000000100000100bb802f0000007809b7aed5b7acd844124ead0e6d35e9fbb84000028200000007265666572656e6365206c6962464c414320312e352e3020323032353032313100000000fff87a1800077fb100000000000019cb";
    HEX.as_bytes()
        .chunks_exact(2)
        .map(|pair| {
            let digits =
                std::str::from_utf8(pair).map_err(|_error| Error::InvalidField("test FLAC"))?;
            u8::from_str_radix(digits, 16).map_err(|_error| Error::InvalidField("test FLAC"))
        })
        .collect()
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
