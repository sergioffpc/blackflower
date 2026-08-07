use std::sync::Arc;
use std::sync::atomic::{AtomicU32, AtomicU64, Ordering};
use std::sync::{Mutex, MutexGuard};

use blackflower_acoustics::{AudibleSoundDelivery, AudibleVoiceDelivery};
use blackflower_ecs::{Error, PhaseId, RunError, TickDelta, World};
use blackflower_rendering::{LatestFrameMailbox, MailboxError, RenderFrame, RenderFrameId};

use crate::audio::{
    AudioCommand, AudioCommandBatch, PresentationAudioError, PresentationAudioState,
};
use crate::movement::{
    MovementProxy, PresentationMovementError, PresentationMovementSample, PresentationMovementState,
};
use crate::telemetry;
use crate::telemetry::FrameObservation;
use crate::{FrameIndex, PresentationPhase, PresentationPipeline, systems};

#[derive(Debug)]
struct ExecutionState {
    frame: AtomicU64,
    delta_seconds: AtomicU32,
    audio: Mutex<PresentationAudioState>,
    movement: Mutex<PresentationMovementState>,
    render: Mutex<PresentationRenderState>,
    render_mailbox: Arc<LatestFrameMailbox>,
}

#[derive(Debug, Default)]
struct PresentationRenderState {
    staged: Option<RenderFrame>,
}

/// Snapshot of the presentation execution visible to registered systems.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub struct FrameExecution {
    /// Frame currently being executed.
    pub frame: FrameIndex,
}

/// Shared read-only execution context that presentation systems may capture.
///
/// The delta for the active frame is available through each system's
/// [`SystemContext`](blackflower_ecs::SystemContext).
#[derive(Debug, Clone)]
pub struct FrameExecutionContext {
    state: Arc<ExecutionState>,
}

impl FrameExecutionContext {
    fn new() -> Self {
        Self {
            state: Arc::new(ExecutionState {
                frame: AtomicU64::new(FrameIndex::ZERO.get()),
                delta_seconds: AtomicU32::new(0.0_f32.to_bits()),
                audio: Mutex::new(PresentationAudioState::default()),
                movement: Mutex::new(PresentationMovementState::default()),
                render: Mutex::new(PresentationRenderState::default()),
                render_mailbox: Arc::new(LatestFrameMailbox::default()),
            }),
        }
    }

    /// Return the frame prepared for the current pipeline invocation.
    #[must_use]
    pub fn current(&self) -> FrameExecution {
        FrameExecution {
            frame: FrameIndex::new(self.state.frame.load(Ordering::Acquire)),
        }
    }

    fn set(&self, execution: FrameExecution) {
        self.state
            .frame
            .store(execution.frame.get(), Ordering::Release);
    }

    fn delta_seconds(&self) -> f32 {
        f32::from_bits(self.state.delta_seconds.load(Ordering::Acquire))
    }

    fn set_delta_seconds(&self, delta_seconds: f32) {
        self.state
            .delta_seconds
            .store(delta_seconds.to_bits(), Ordering::Release);
    }

    fn audio(&self) -> Result<MutexGuard<'_, PresentationAudioState>, PresentationAudioError> {
        self.state
            .audio
            .lock()
            .map_err(|_error| PresentationAudioError::Unavailable)
    }

    fn movement(
        &self,
    ) -> Result<MutexGuard<'_, PresentationMovementState>, PresentationMovementError> {
        self.state
            .movement
            .lock()
            .map_err(|_error| PresentationMovementError::StateUnavailable)
    }

    pub(crate) fn reset_frame_transient(&self) -> Result<(), PresentationOutputError> {
        self.audio()?.reset_transient();
        self.movement()?.begin_frame();
        self.state
            .render
            .lock()
            .map_err(|_error| PresentationOutputError::RenderStateUnavailable)?
            .staged = None;
        Ok(())
    }

    pub(crate) fn capture_local_movement(&self) -> Result<(), PresentationMovementError> {
        self.movement()?.capture();
        Ok(())
    }

    pub(crate) fn create_missing_movement_proxy(&self) -> Result<(), PresentationMovementError> {
        self.movement()?.create_missing_proxy();
        Ok(())
    }

    pub(crate) fn retire_stale_movement_proxy(&self) -> Result<(), PresentationMovementError> {
        self.movement()?.retire_stale_proxy();
        Ok(())
    }

    pub(crate) fn sample_local_prediction(&self) -> Result<(), PresentationMovementError> {
        self.movement()?.sample_prediction();
        Ok(())
    }

    pub(crate) fn smooth_movement_correction(&self) -> Result<(), PresentationMovementError> {
        self.movement()?.smooth_correction(self.delta_seconds());
        Ok(())
    }

    pub(crate) fn release_captured_movement(&self) -> Result<(), PresentationMovementError> {
        self.movement()?.release_captured();
        Ok(())
    }

    fn commit_movement_frame(&self) -> Result<(), PresentationMovementError> {
        self.movement()?.commit_frame();
        Ok(())
    }

    fn discard_movement_frame(&self) -> Result<(), PresentationMovementError> {
        self.movement()?.discard_frame();
        Ok(())
    }

    pub(crate) fn update_audio_emitters(&self) -> Result<(), PresentationAudioError> {
        self.audio()?.update_emitters();
        Ok(())
    }

    pub(crate) fn build_audio_commands(&self) -> Result<(), PresentationAudioError> {
        self.audio()?.build();
        Ok(())
    }

    pub(crate) fn publish_audio_commands(&self) -> Result<(), PresentationAudioError> {
        self.audio()?.publish(self.current().frame);
        Ok(())
    }

    pub(crate) fn build_render_frame(&self) -> Result<(), PresentationOutputError> {
        let frame = RenderFrame::empty(RenderFrameId::new(self.current().frame.get()));
        self.state
            .render
            .lock()
            .map_err(|_error| PresentationOutputError::RenderStateUnavailable)?
            .staged = Some(frame);
        Ok(())
    }

    pub(crate) fn publish_render_frame(&self) -> Result<(), PresentationOutputError> {
        let frame = self
            .state
            .render
            .lock()
            .map_err(|_error| PresentationOutputError::RenderStateUnavailable)?
            .staged
            .take()
            .ok_or(PresentationOutputError::RenderFrameNotBuilt)?;
        let _outcome = self.state.render_mailbox.publish(frame)?;
        Ok(())
    }
}

/// Failure while building or publishing immutable presentation outputs.
#[derive(Debug, Clone, Copy, PartialEq, Eq, thiserror::Error)]
pub enum PresentationOutputError {
    /// Presentation-owned render staging was poisoned by a previous panic.
    #[error("presentation render output state is unavailable")]
    RenderStateUnavailable,
    /// `PublishRenderFrame` ran without a frame built for this attempt.
    #[error("render frame was not built before publication")]
    RenderFrameNotBuilt,
    /// The renderer handoff rejected access.
    #[error(transparent)]
    Mailbox(#[from] MailboxError),
    /// Presentation-owned audio command state rejected access.
    #[error(transparent)]
    Audio(#[from] PresentationAudioError),
    /// Presentation-owned local movement state rejected access.
    #[error(transparent)]
    Movement(#[from] PresentationMovementError),
}

/// Failure while advancing a presentation frame.
#[derive(Debug, Clone, PartialEq, Eq, thiserror::Error)]
pub enum PresentationError {
    /// A registered ECS system failed.
    #[error(transparent)]
    Run(#[from] RunError),
    /// The monotonic presentation frame index exhausted its representation.
    #[error("presentation frame index overflow")]
    FrameIndexOverflow,
    /// Presentation-owned movement state could not commit or roll back.
    #[error(transparent)]
    Movement(#[from] PresentationMovementError),
}

/// Dedicated ECS world for variable-step client presentation.
///
/// Construction registers the complete [`PresentationPipeline`]. The owner may
/// then register client-only components and systems through [`Self::ecs_mut`]
/// before advancing the pipeline once per displayed frame with [`Self::frame`].
///
/// Wall-clock pacing, input collection, prediction, reconciliation, transport,
/// and concrete output backends stay outside this type. Captured simulation
/// state is read-only; systems may mutate only presentation-owned state.
pub struct PresentationWorld {
    ecs: World,
    pipeline: PresentationPipeline,
    current_frame: FrameIndex,
    execution_context: FrameExecutionContext,
}

impl PresentationWorld {
    /// Create a single-threaded presentation world.
    pub fn new() -> Result<Self, Error> {
        Self::from_ecs(World::new()?)
    }

    /// Turn an existing, independently configured ECS world into a presentation world.
    pub fn from_ecs(mut ecs: World) -> Result<Self, Error> {
        let pipeline = PresentationPipeline::register(&mut ecs)?;
        let execution_context = FrameExecutionContext::new();
        systems::register(&mut ecs, pipeline, execution_context.clone())?;
        telemetry::describe_metrics();
        Ok(Self {
            ecs,
            pipeline,
            current_frame: FrameIndex::ZERO,
            execution_context,
        })
    }

    /// Return the underlying ECS world.
    #[must_use]
    pub const fn ecs(&self) -> &World {
        &self.ecs
    }

    /// Return the underlying ECS world for setup or direct client-only state access.
    #[must_use]
    pub const fn ecs_mut(&mut self) -> &mut World {
        &mut self.ecs
    }

    /// Return the registered presentation pipeline.
    #[must_use]
    pub const fn pipeline(&self) -> PresentationPipeline {
        self.pipeline
    }

    /// Return the world-bound handle for one presentation phase.
    #[must_use]
    pub const fn phase(&self, phase: PresentationPhase) -> PhaseId {
        self.pipeline.phase(phase)
    }

    /// Return the latest successfully completed presentation frame.
    #[must_use]
    pub const fn current_frame(&self) -> FrameIndex {
        self.current_frame
    }

    /// Return a context handle for systems that need the active frame index.
    #[must_use]
    pub fn execution_context(&self) -> FrameExecutionContext {
        self.execution_context.clone()
    }

    /// Return the single-slot latest-frame handoff consumed by the renderer thread.
    #[must_use]
    pub fn render_mailbox(&self) -> Arc<LatestFrameMailbox> {
        Arc::clone(&self.execution_context.state.render_mailbox)
    }

    /// Replace the local movement sample captured by the next presentation frame.
    ///
    /// Passing `None` retires the current local movement proxy. The copied
    /// sample never grants presentation access to prediction-owned state.
    pub fn set_local_movement_sample(
        &self,
        sample: Option<PresentationMovementSample>,
    ) -> Result<(), PresentationMovementError> {
        self.execution_context.movement()?.set_pending(sample);
        Ok(())
    }

    /// Return the local movement proxy from the latest successful frame.
    pub fn local_movement_proxy(&self) -> Result<Option<MovementProxy>, PresentationMovementError> {
        Ok(self.execution_context.movement()?.committed())
    }

    /// Queue one server-authorized remote sound for the next presentation frame.
    pub fn queue_audible_sound(
        &self,
        delivery: AudibleSoundDelivery,
    ) -> Result<(), PresentationAudioError> {
        self.execution_context.audio()?.queue_sound(delivery);
        Ok(())
    }

    /// Queue one server-gated physical-world voice packet for the next frame.
    pub fn queue_audible_voice(
        &self,
        delivery: AudibleVoiceDelivery,
    ) -> Result<(), PresentationAudioError> {
        self.execution_context.audio()?.queue_voice(delivery);
        Ok(())
    }

    /// Drain immutable audio commands after `PublishAudioCommands` accepts them.
    pub fn drain_submitted_audio_commands(
        &self,
    ) -> Result<Vec<AudioCommand>, PresentationAudioError> {
        Ok(self.execution_context.audio()?.drain_submitted())
    }

    /// Drain frame-keyed immutable audio batches in presentation order.
    pub fn drain_submitted_audio_batches(
        &self,
    ) -> Result<Vec<AudioCommandBatch>, PresentationAudioError> {
        Ok(self.execution_context.audio()?.drain_submitted_batches())
    }

    /// Advance the presentation pipeline by one frame using a validated delta.
    ///
    /// The frame index is committed only after every phase succeeds. In
    /// particular, a failure in `PublishFrameOutputs` prevents
    /// `CommitFrameHistory` from running and leaves the frame index unchanged.
    #[cfg_attr(
        feature = "tracing",
        tracing::instrument(
            target = "blackflower_world_presentation",
            name = "presentation_frame",
            level = "info",
            skip_all,
            fields(
                frame = tracing::field::Empty,
                delta_seconds = f64::from(delta.as_seconds()),
                result = tracing::field::Empty,
                reason = tracing::field::Empty,
            ),
        )
    )]
    pub fn frame(&mut self, delta: TickDelta) -> Result<bool, PresentationError> {
        let Some(next_frame) = self.current_frame.checked_next() else {
            telemetry::frame_rejected("frame_index_overflow");
            return Err(PresentationError::FrameIndexOverflow);
        };
        #[cfg(feature = "tracing")]
        tracing::Span::current().record("frame", next_frame.get());
        let previous_execution = self.execution_context.current();
        let previous_delta_seconds = self.execution_context.delta_seconds();
        self.execution_context
            .set(FrameExecution { frame: next_frame });
        self.execution_context.set_delta_seconds(delta.as_seconds());

        let observation = FrameObservation::start(delta);
        let run_result = self.ecs.progress(delta);
        let result = match run_result {
            Ok(should_continue) => {
                if let Err(error) = self.execution_context.commit_movement_frame() {
                    self.execution_context.set(previous_execution);
                    self.execution_context
                        .set_delta_seconds(previous_delta_seconds);
                    Err(PresentationError::Movement(error))
                } else {
                    self.current_frame = next_frame;
                    Ok(should_continue)
                }
            }
            Err(error) => {
                self.execution_context.set(previous_execution);
                self.execution_context
                    .set_delta_seconds(previous_delta_seconds);
                match self.execution_context.discard_movement_frame() {
                    Ok(()) => Err(PresentationError::Run(error)),
                    Err(movement_error) => Err(PresentationError::Movement(movement_error)),
                }
            }
        };
        observation.finish(&result);

        #[cfg(feature = "profiling")]
        profiling::finish_frame!();

        result
    }
}
