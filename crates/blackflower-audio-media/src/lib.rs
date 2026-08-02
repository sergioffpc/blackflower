#![doc = include_str!("../README.md")]

mod clip;
mod cooker;
mod error;
mod event;
mod library;
mod stream;

pub use clip::{AudioClip, LoopRegion};
pub use cooker::{AudioCookSettings, cook_clip, cook_stream};
pub use error::Error;
pub use event::{Attenuation, Concurrency, SoundEvent, Spatialization};
pub use library::{AudioAsset, AudioLibrary};
pub use stream::{AudioFrame, AudioStream, AudioStreamDecoder};

/// Runtime sample rate used by every cooked Blackflower audio asset.
pub const AUDIO_SAMPLE_RATE: u32 = 48_000;

/// Current `.bfaudio` container schema.
pub const AUDIO_CLIP_SCHEMA: u32 = 1;

/// Current `.bfsound` container schema.
pub const SOUND_EVENT_SCHEMA: u32 = 1;

/// Stable audio cooking algorithm identity.
pub const COOKER_RECIPE: &str = "audio-media-v2;pcm16-le;flac-stream-pass-through";

/// Pinned authored WAV decoder.
pub const HOUND_VERSION: &str = "3.5.1";

/// Pinned authored FLAC decoder.
pub const CLAXON_VERSION: &str = "0.4.3";

/// Pinned offline resampler.
pub const RUBATO_VERSION: &str = "4.0.0";
