/// Microphone capture, worker, or voice-analysis failure.
#[derive(Debug, thiserror::Error)]
pub enum Error {
    /// A public capture setting is invalid.
    #[error("invalid capture setting `{0}`")]
    InvalidSetting(&'static str),
    /// No default input device is available.
    #[error("no default input device is available")]
    NoInputDevice,
    /// The default input sample representation is unsupported.
    #[error("unsupported input sample format")]
    UnsupportedSampleFormat,
    /// CPAL rejected device discovery, stream construction, or stream state.
    #[error("audio input device failed: {0}")]
    Device(String),
    /// The capture stream has already transferred its single consumer worker.
    #[error("capture worker has already been taken")]
    WorkerTaken,
    /// The configured server analyzer pool has no free sender slot.
    #[error("voice analyzer sender capacity was reached")]
    AnalyzerCapacity,
    /// A mock-only operation was attempted on a real CPAL stream.
    #[error("mock capture input is unavailable on a real stream")]
    NotMock,
    /// Opus rejected encoder or decoder state.
    #[error(transparent)]
    Opus(#[from] blackflower_audio_voice::Error),
    /// The bounded encoded packet or acoustic value is invalid.
    #[error(transparent)]
    Acoustic(#[from] blackflower_acoustics::Error),
}
