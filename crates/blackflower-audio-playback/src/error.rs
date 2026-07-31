use thiserror::Error;

/// Playback engine failure.
#[derive(Debug, Error)]
pub enum Error {
    #[error("audio device initialization failed: {0}")]
    Device(String),
    #[error("audio resource capacity was reached")]
    ResourceLimit,
    #[error("audio media failed: {0}")]
    Media(#[from] blackflower_audio_media::Error),
    #[error("audio asset `{0}` is missing or has the wrong kind")]
    MissingAsset(blackflower_assets::AssetId),
    #[error("voice was rejected by priority or concurrency policy")]
    VoiceRejected,
    #[error("voice ID is unknown")]
    UnknownVoice,
    #[error("invalid playback field `{0}`")]
    InvalidField(&'static str),
}
