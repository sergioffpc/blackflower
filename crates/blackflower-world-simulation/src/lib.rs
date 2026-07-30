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
    AI_UPDATE_INTERVAL_TICKS, AI_UPDATE_RATE_HZ, CONTROL_FRAME_INTERVAL_TICKS,
    CONTROL_FRAME_RATE_HZ, INPUT_TIMEOUT_TICKS, SIMULATION_TICK_RATE_HZ, SNAPSHOT_INTERVAL_TICKS,
    SNAPSHOT_RATE_HZ, SimulationPhase, SimulationPhases, SimulationPipeline,
};
pub use systems::{
    CaptureTickInputsSystem, CommitStateTransitionsSystem, DeriveActorActionsSystem,
    DeriveStateTransitionsSystem, EmitBotControlFramesSystem, PlanBotTacticsSystem,
    PrepareTickSystem, SealTickSystem, SolveAcousticsSystem, SolvePhysicalPhenomenaSystem,
    SolveRigidBodyDynamicsSystem, SubmitTickOutputsSystem, UpdateBotPerceptionSystem,
    UpdateSpatialStructuresSystem,
};
pub use types::SimulationTick;
pub use world::{
    SIMULATION_TICK_DELTA_SECONDS, SimulationExecution, SimulationExecutionContext, SimulationWorld,
};
