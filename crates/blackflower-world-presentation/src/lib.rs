//! Variable-step client presentation world and its ordered frame phases.
//!
//! Client-only systems register themselves in one of the phases exposed by
//! [`PresentationPipeline`]. [`PresentationWorld`] owns a dedicated ECS world
//! and advances that pipeline once for each displayed frame.
//!
//! Prediction, reconciliation, transport, and concrete rendering or audio
//! backends remain outside this crate. Presentation systems may mutate their
//! own client-only state, but must treat captured simulation state as immutable.

mod audio;
mod pipeline;
mod systems;
mod telemetry;
mod types;
mod world;

pub use audio::{AudioCommand, PresentationAudioError};
pub use pipeline::{PresentationPhase, PresentationPhases, PresentationPipeline};
pub use systems::{
    BuildBackendCommandsSystem, CaptureFrameInputsSystem, CommitFrameHistorySystem,
    EvaluateAnimationPosesSystem, PrepareFrameSystem, ResolveSceneGraphSystem,
    SampleRenderTimelineSystem, SubmitBackendCommandsSystem, UpdateCamerasAndListenersSystem,
    UpdateEffectsAndFeedbackSystem, UpdateSceneProxiesSystem,
};
pub use types::FrameIndex;
pub use world::{FrameExecution, FrameExecutionContext, PresentationError, PresentationWorld};
