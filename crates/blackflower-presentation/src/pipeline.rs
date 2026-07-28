use blackflower_ecs::{BuiltinPhase, Error, PhaseId, World};
use strum::IntoStaticStr;

/// A synchronization phase in one client presentation frame.
///
/// [`PresentationPhase::ORDER`] is the normative execution order. A phase is a
/// data-availability boundary rather than a backend subsystem: systems in
/// earlier phases prepare stable client-only state for systems in later phases.
#[derive(Debug, Clone, Copy, PartialEq, Eq, PartialOrd, Ord, Hash, IntoStaticStr)]
pub enum PresentationPhase {
    /// Prepare the frame index, timing context, and transient storage.
    PrepareFrame,
    /// Capture the immutable predicted state, snapshots, events, and settings.
    CaptureFrameInputs,
    /// Create, update, and retire client-only proxies for simulation entities.
    UpdateSceneProxies,
    /// Sample local prediction and remote interpolation at the render time.
    SampleRenderTimeline,
    /// Update active cameras, viewports, and spatial-audio listeners.
    UpdateCamerasAndListeners,
    /// Evaluate animation graphs, inverse kinematics, and procedural poses.
    EvaluateAnimationPoses,
    /// Resolve hierarchies, bones, sockets, attachments, and world transforms.
    ResolveSceneGraph,
    /// Advance visual effects, audio, user interface, and haptic feedback.
    UpdateEffectsAndFeedback,
    /// Build immutable commands for the active client backends.
    BuildBackendCommands,
    /// Submit frame commands to rendering, audio, UI, and haptic backends.
    SubmitBackendCommands,
    /// Commit previous-frame state and retire consumed inputs and events.
    CommitFrameHistory,
}

impl PresentationPhase {
    /// Number of phases in the presentation pipeline.
    pub const COUNT: usize = 11;

    /// Normative execution order of presentation phases.
    pub const ORDER: [Self; Self::COUNT] = [
        Self::PrepareFrame,
        Self::CaptureFrameInputs,
        Self::UpdateSceneProxies,
        Self::SampleRenderTimeline,
        Self::UpdateCamerasAndListeners,
        Self::EvaluateAnimationPoses,
        Self::ResolveSceneGraph,
        Self::UpdateEffectsAndFeedback,
        Self::BuildBackendCommands,
        Self::SubmitBackendCommands,
        Self::CommitFrameHistory,
    ];

    /// Stable scheduler entity name for this phase.
    #[must_use]
    pub fn name(self) -> &'static str {
        self.into()
    }
}

/// World-bound handles for every presentation phase.
#[derive(Debug, Clone, Copy)]
pub struct PresentationPhases {
    prepare_frame: PhaseId,
    capture_frame_inputs: PhaseId,
    update_scene_proxies: PhaseId,
    sample_render_timeline: PhaseId,
    update_cameras_and_listeners: PhaseId,
    evaluate_animation_poses: PhaseId,
    resolve_scene_graph: PhaseId,
    update_effects_and_feedback: PhaseId,
    build_backend_commands: PhaseId,
    submit_backend_commands: PhaseId,
    commit_frame_history: PhaseId,
}

impl PresentationPhases {
    fn register(world: &mut World) -> Result<Self, Error> {
        let [
            prepare_frame,
            capture_frame_inputs,
            update_scene_proxies,
            sample_render_timeline,
            update_cameras_and_listeners,
            evaluate_animation_poses,
            resolve_scene_graph,
            update_effects_and_feedback,
            build_backend_commands,
            submit_backend_commands,
            commit_frame_history,
        ] = register_phase_chain(world)?;
        Ok(Self {
            prepare_frame,
            capture_frame_inputs,
            update_scene_proxies,
            sample_render_timeline,
            update_cameras_and_listeners,
            evaluate_animation_poses,
            resolve_scene_graph,
            update_effects_and_feedback,
            build_backend_commands,
            submit_backend_commands,
            commit_frame_history,
        })
    }

    /// Return the world-bound handle for one presentation phase.
    #[must_use]
    pub const fn get(self, phase: PresentationPhase) -> PhaseId {
        match phase {
            PresentationPhase::PrepareFrame => self.prepare_frame,
            PresentationPhase::CaptureFrameInputs => self.capture_frame_inputs,
            PresentationPhase::UpdateSceneProxies => self.update_scene_proxies,
            PresentationPhase::SampleRenderTimeline => self.sample_render_timeline,
            PresentationPhase::UpdateCamerasAndListeners => self.update_cameras_and_listeners,
            PresentationPhase::EvaluateAnimationPoses => self.evaluate_animation_poses,
            PresentationPhase::ResolveSceneGraph => self.resolve_scene_graph,
            PresentationPhase::UpdateEffectsAndFeedback => self.update_effects_and_feedback,
            PresentationPhase::BuildBackendCommands => self.build_backend_commands,
            PresentationPhase::SubmitBackendCommands => self.submit_backend_commands,
            PresentationPhase::CommitFrameHistory => self.commit_frame_history,
        }
    }
}

/// Registered variable-step client presentation phases.
///
/// Register client-only systems against [`Self::phase`], then advance the
/// dedicated presentation world with [`PresentationWorld::frame`](crate::PresentationWorld::frame).
#[derive(Debug, Clone, Copy)]
pub struct PresentationPipeline {
    phases: PresentationPhases,
}

impl PresentationPipeline {
    /// Register all presentation phases in `world`.
    pub fn register(world: &mut World) -> Result<Self, Error> {
        let phases = PresentationPhases::register(world)?;
        Ok(Self { phases })
    }

    /// Return all world-bound phase handles.
    #[must_use]
    pub const fn phases(self) -> PresentationPhases {
        self.phases
    }

    /// Return the world-bound handle for one presentation phase.
    #[must_use]
    pub const fn phase(self, phase: PresentationPhase) -> PhaseId {
        self.phases.get(phase)
    }
}

fn register_phase_chain(world: &mut World) -> Result<[PhaseId; PresentationPhase::COUNT], Error> {
    let first_phase = PresentationPhase::PrepareFrame;
    let first = world.create_phase(
        first_phase.name(),
        Some(world.builtin_phase(BuiltinPhase::OnUpdate)),
    )?;
    let mut registered = [first; PresentationPhase::COUNT];
    let mut previous = first;
    for (slot, phase) in registered
        .iter_mut()
        .skip(1)
        .zip(PresentationPhase::ORDER.into_iter().skip(1))
    {
        let current = create_phase_after(world, phase, previous)?;
        *slot = current;
        previous = current;
    }
    Ok(registered)
}

fn create_phase_after(
    world: &mut World,
    phase: PresentationPhase,
    previous: PhaseId,
) -> Result<PhaseId, Error> {
    world.create_phase(phase.name(), Some(previous))
}
