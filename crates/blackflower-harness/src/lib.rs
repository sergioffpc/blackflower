#![doc = include_str!("../README.md")]

mod client;
mod input;
mod prediction;
mod snapshots;
mod trace;
mod transport;
mod types;

pub use blackflower_networking::ControlBinding;
pub use blackflower_world_prediction::{
    AbsoluteTolerance, AngularTolerance, PredictionDriver, PredictionPass,
    PredictionStateComparison, ToleranceError,
};
pub use client::{ClientHarness, ClientHarnessError};
pub use input::InputBuildError as ClientInputError;
pub use prediction::{
    ClientPrediction, PredictionCodec, PredictionSession, PredictionSessionError, PredictionUpdate,
};
pub use snapshots::{SnapshotInboxError as ClientSnapshotError, SnapshotWindow};
pub use trace::{TraceObserver, TraceRecord};
pub use transport::{ClientTransport, ClientTransportEvent};
pub use types::{
    ClientEvent, ClientHarnessConfig, ClientView, CommandSubmission, ControlSubmission,
};
