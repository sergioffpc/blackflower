#![doc = include_str!("../README.md")]

mod client;
mod input;
mod prediction;
mod snapshots;
mod transport;
mod types;

pub use client::{ClientHarness, ClientHarnessError};
pub use input::InputBuildError as ClientInputError;
pub use prediction::{
    ClientPrediction, ForwardPredictionDriver, PredictionCodec, PredictionSession,
    PredictionSessionError, PredictionUpdate,
};
pub use snapshots::{SnapshotInboxError as ClientSnapshotError, SnapshotWindow};
pub use transport::{ClientTransport, ClientTransportEvent};
pub use types::{
    ClientEvent, ClientHarnessConfig, ClientView, CommandSubmission, ControlBinding,
    ControlSubmission,
};
