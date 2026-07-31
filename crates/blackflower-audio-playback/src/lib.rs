#![doc = include_str!("../README.md")]

mod decoder;
mod engine;
mod error;
mod hrtf;

pub use engine::{AudioEngine, AudioEngineSettings, AudioEvent, PlaybackParams, VoiceId};
pub use error::Error;

/// Kira version used behind the Blackflower playback boundary.
pub const KIRA_VERSION: &str = "0.12.2";

/// Fixed Kira render quantum used by Blackflower.
pub const INTERNAL_BUFFER_SIZE: usize = 256;
