#![doc = include_str!("../README.md")]

mod decoder;
mod engine;
mod error;
mod hrtf;
mod live_voice;

pub use engine::{AudioEngine, AudioEngineSettings, AudioEvent, PlaybackParams, VoiceId};
pub use error::Error;
pub use live_voice::{DecodedVoiceFrame, RemoteVoiceJitterBuffer};

/// Kira version used behind the Blackflower playback boundary.
pub const KIRA_VERSION: &str = "0.12.2";

/// Fixed Kira render quantum used by Blackflower.
pub const INTERNAL_BUFFER_SIZE: usize = 256;
