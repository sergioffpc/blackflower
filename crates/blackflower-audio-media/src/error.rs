use thiserror::Error;

/// Audio media validation, cooking, or decoding failure.
#[derive(Debug, Error)]
pub enum Error {
    #[error("invalid audio container: {0}")]
    InvalidContainer(&'static str),
    #[error("unsupported audio source: {0}")]
    UnsupportedSource(&'static str),
    #[error("unsupported channel count {0}; only mono and stereo are accepted")]
    UnsupportedChannels(u32),
    #[error("audio source contains no frames")]
    Empty,
    #[error("audio field `{0}` is invalid")]
    InvalidField(&'static str),
    #[error("WAV decoder failed: {0}")]
    Wav(#[from] hound::Error),
    #[error("FLAC decoder failed: {0}")]
    Flac(#[from] claxon::Error),
    #[error("resampler failed: {0}")]
    Resample(String),
    #[error("asset ID is invalid: {0}")]
    AssetId(#[from] blackflower_assets::InvalidAssetId),
}
