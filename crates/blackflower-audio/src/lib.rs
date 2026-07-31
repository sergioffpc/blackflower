#![doc = include_str!("../README.md")]

/// Lock-free microphone capture and off-callback voice analysis.
pub use blackflower_audio_capture as capture;

/// Cooked clip, stream, event, and library formats.
pub use blackflower_audio_media as media;

/// Device playback, mixing, HRTF tracks, and voice policy.
pub use blackflower_audio_playback as playback;

/// Spatial audio processing backed by Steam Audio.
pub use blackflower_audio_spatial as spatial;

/// Real-time voice encoding and decoding backed by Opus.
pub use blackflower_audio_voice as voice;
