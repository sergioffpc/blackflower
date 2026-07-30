//! Fixed-step client prediction and authoritative reconciliation.
//!
//! [`PredictionPipeline`] is the only ECS pipeline in this crate. It advances
//! one predicted tick in either [`PredictionPass::Forward`] or
//! [`PredictionPass::Resimulation`]. [`ReconciliationCoordinator`] remains
//! ordinary Rust control flow: it restores an authoritative baseline and asks a
//! simulation-provided [`ReconciliationDriver`] to run the same prediction pipeline
//! for each recorded input that follows the baseline.

mod history;
mod pipeline;
mod reconciliation;
mod systems;
mod telemetry;
mod types;
mod world;

pub use history::{HistoryError, InputFrame, InputHistory, PredictionFrame, PredictionHistory};
pub use pipeline::{PredictionPhase, PredictionPhases, PredictionPipeline};
pub use reconciliation::{
    AuthoritativeSnapshot, HardResyncReason, ReconciliationCoordinator, ReconciliationDriver,
    ReconciliationError, ReconciliationOutcome,
};
pub use systems::{
    CaptureTickInputsSystem, CommitStateTransitionsSystem, DeriveActorActionsSystem,
    DeriveStateTransitionsSystem, PrepareTickSystem, SealTickSystem, SolveRigidBodyDynamicsSystem,
    SubmitTickOutputsSystem,
};
pub use types::{InputSequence, PredictionPass, PredictionTick};
pub use world::{
    PREDICTION_TICK_DELTA_SECONDS, PREDICTION_TICK_RATE_HZ, PredictionError, PredictionExecution,
    PredictionExecutionContext, PredictionWorld,
};
