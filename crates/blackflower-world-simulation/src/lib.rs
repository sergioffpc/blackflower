//! Authoritative fixed-tick simulation world and its ordered phases.
//!
//! Gameplay systems register themselves in one of the phases exposed by
//! [`SimulationPipeline`]. [`SimulationWorld`] owns the ECS world and advances
//! that pipeline at the fixed simulation delta.

mod pipeline;
mod systems;
mod telemetry;
mod types;
mod world;

pub use pipeline::{
    CONTROL_FRAME_INTERVAL_TICKS, CONTROL_FRAME_RATE_HZ, INPUT_FAILSAFE_TICKS, INPUT_GRACE_TICKS,
    SIMULATION_TICK_RATE_HZ, SNAPSHOT_INTERVAL_TICKS, SNAPSHOT_RATE_HZ, SimulationPhase,
    SimulationPhases, SimulationPipeline,
};
pub use systems::{
    CaptureTickInputsSystem, CommitStateTransitionsSystem, DeriveActorActionsSystem,
    DeriveStateTransitionsSystem, PrepareTickSystem, SealTickSystem, SolveAcousticsSystem,
    SolvePhysicalPhenomenaSystem, SolveRigidBodyDynamicsSystem, SubmitTickOutputsSystem,
    UpdateSpatialStructuresSystem,
};
pub use types::SimulationTick;
pub use world::{
    AcousticRuntimeError, SIMULATION_TICK_DELTA_SECONDS, SimulationExecution,
    SimulationExecutionContext, SimulationWorld,
};
