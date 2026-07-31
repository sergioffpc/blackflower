#![doc = include_str!("../README.md")]

mod analyzer;
mod error;
mod ring;
mod stream;
mod worker;

pub use analyzer::{VoiceAcousticFrame, VoiceAnalyzerBank, VoiceFrameAnalyzer};
pub use error::Error;
pub use stream::{CaptureSettings, CaptureStream, VoiceActivation};
pub use worker::{CapturedVoiceFrame, VoiceCaptureWorker};
