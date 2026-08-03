use blackflower_ecs::{Error, PhaseId, SystemResult, Tag, World};
use strum::IntoStaticStr;

use crate::{FrameExecutionContext, PresentationPhase, PresentationPipeline, telemetry};

#[derive(Tag)]
struct PresentationSystemDriver;

type SystemCallback = fn(&FrameExecutionContext) -> SystemResult;

/// A system in the client [`PresentationPhase::PrepareFrame`] phase.
///
/// These systems open the frame-local working context and reset transient
/// storage before immutable frame inputs are captured.
#[derive(Debug, Clone, Copy, PartialEq, Eq, PartialOrd, Ord, Hash, IntoStaticStr)]
pub enum PrepareFrameSystem {
    /// Open the frame-local working context prepared by `PresentationWorld::frame`.
    OpenFrame,
    /// Reset scratch storage and command staging left by the previous frame attempt.
    ResetFrameTransientStorage,
    /// Resolve bounded frame timing, pause, and discontinuity policy.
    ResolveFrameTiming,
}

impl PrepareFrameSystem {
    /// Number of systems in `PrepareFrame`.
    pub const COUNT: usize = 3;

    /// Stable registration order for `PrepareFrame` systems.
    pub const ORDER: [Self; Self::COUNT] = [
        Self::OpenFrame,
        Self::ResetFrameTransientStorage,
        Self::ResolveFrameTiming,
    ];

    /// Stable scheduler entity and trace field name for this system.
    #[must_use]
    pub fn name(self) -> &'static str {
        self.into()
    }

    fn callback(self) -> SystemCallback {
        match self {
            Self::OpenFrame => open_frame,
            Self::ResetFrameTransientStorage => reset_frame_transient_storage,
            Self::ResolveFrameTiming => resolve_frame_timing,
        }
    }

    pub(crate) fn register(
        self,
        world: &mut World,
        phase: PhaseId,
        driver_expression: &'static str,
        execution_context: FrameExecutionContext,
    ) -> Result<(), Error> {
        register_system(
            world,
            phase,
            driver_expression,
            PresentationPhase::PrepareFrame,
            self.name(),
            execution_context,
            self.callback(),
        )
    }
}

/// A system in the client [`PresentationPhase::CaptureFrameInputs`] phase.
///
/// These systems capture stable, read-only inputs for the active frame without
/// advancing or mutating their prediction and simulation sources.
#[derive(Debug, Clone, Copy, PartialEq, Eq, PartialOrd, Ord, Hash, IntoStaticStr)]
pub enum CaptureFrameInputsSystem {
    /// Capture one immutable session, authoritative, and predicted client view.
    CaptureClientView,
    /// Capture the bounded authoritative projection window required for interpolation.
    CaptureInterpolationWindow,
    /// Capture available client-facing events without consuming their source.
    CaptureClientEvents,
    /// Capture the active view and output configuration for this frame.
    CaptureFrameConfiguration,
    /// Capture immutable renderer, audio, UI, and haptic backend feedback.
    CaptureBackendFeedback,
}

impl CaptureFrameInputsSystem {
    /// Number of systems in `CaptureFrameInputs`.
    pub const COUNT: usize = 5;

    /// Stable registration order for `CaptureFrameInputs` systems.
    pub const ORDER: [Self; Self::COUNT] = [
        Self::CaptureClientView,
        Self::CaptureInterpolationWindow,
        Self::CaptureClientEvents,
        Self::CaptureFrameConfiguration,
        Self::CaptureBackendFeedback,
    ];

    /// Stable scheduler entity and trace field name for this system.
    #[must_use]
    pub fn name(self) -> &'static str {
        self.into()
    }

    fn callback(self) -> SystemCallback {
        match self {
            Self::CaptureClientView => capture_client_view,
            Self::CaptureInterpolationWindow => capture_interpolation_window,
            Self::CaptureClientEvents => capture_client_events,
            Self::CaptureFrameConfiguration => capture_frame_configuration,
            Self::CaptureBackendFeedback => capture_backend_feedback,
        }
    }

    pub(crate) fn register(
        self,
        world: &mut World,
        phase: PhaseId,
        driver_expression: &'static str,
        execution_context: FrameExecutionContext,
    ) -> Result<(), Error> {
        register_system(
            world,
            phase,
            driver_expression,
            PresentationPhase::CaptureFrameInputs,
            self.name(),
            execution_context,
            self.callback(),
        )
    }
}

/// A system in the client [`PresentationPhase::UpdateSceneProxies`] phase.
///
/// These systems maintain presentation-owned proxies for the stable source
/// identities captured at the start of the active frame.
#[derive(Debug, Clone, Copy, PartialEq, Eq, PartialOrd, Ord, Hash, IntoStaticStr)]
pub enum UpdateSceneProxiesSystem {
    /// Create client-only proxies for captured sources that do not have one.
    CreateMissingSceneProxies,
    /// Refresh source identities, resource descriptors, capabilities, and metadata.
    RefreshSceneProxyBindings,
    /// Retire proxies whose captured sources disappeared or expired.
    RetireStaleSceneProxies,
}

impl UpdateSceneProxiesSystem {
    /// Number of systems in `UpdateSceneProxies`.
    pub const COUNT: usize = 3;

    /// Stable registration order for `UpdateSceneProxies` systems.
    pub const ORDER: [Self; Self::COUNT] = [
        Self::CreateMissingSceneProxies,
        Self::RefreshSceneProxyBindings,
        Self::RetireStaleSceneProxies,
    ];

    /// Stable scheduler entity and trace field name for this system.
    #[must_use]
    pub fn name(self) -> &'static str {
        self.into()
    }

    fn callback(self) -> SystemCallback {
        match self {
            Self::CreateMissingSceneProxies => create_missing_scene_proxies,
            Self::RefreshSceneProxyBindings => refresh_scene_proxy_bindings,
            Self::RetireStaleSceneProxies => retire_stale_scene_proxies,
        }
    }

    pub(crate) fn register(
        self,
        world: &mut World,
        phase: PhaseId,
        driver_expression: &'static str,
        execution_context: FrameExecutionContext,
    ) -> Result<(), Error> {
        register_system(
            world,
            phase,
            driver_expression,
            PresentationPhase::UpdateSceneProxies,
            self.name(),
            execution_context,
            self.callback(),
        )
    }
}

/// A system in the client [`PresentationPhase::SampleRenderTimeline`] phase.
///
/// These systems resolve the active sample time and derive client-only state
/// from immutable local prediction and remote snapshot inputs.
#[derive(Debug, Clone, Copy, PartialEq, Eq, PartialOrd, Ord, Hash, IntoStaticStr)]
pub enum SampleRenderTimelineSystem {
    /// Resolve the sample time from frame timing and interpolation configuration.
    ResolveRenderTime,
    /// Sample presentation state for locally predicted entities.
    SampleLocalPrediction,
    /// Interpolate remote entity state from captured snapshot history.
    InterpolateRemoteSnapshots,
    /// Smooth visual corrections produced by completed reconciliation.
    SmoothReconciliationCorrections,
}

impl SampleRenderTimelineSystem {
    /// Number of systems in `SampleRenderTimeline`.
    pub const COUNT: usize = 4;

    /// Stable registration order for `SampleRenderTimeline` systems.
    pub const ORDER: [Self; Self::COUNT] = [
        Self::ResolveRenderTime,
        Self::SampleLocalPrediction,
        Self::InterpolateRemoteSnapshots,
        Self::SmoothReconciliationCorrections,
    ];

    /// Stable scheduler entity and trace field name for this system.
    #[must_use]
    pub fn name(self) -> &'static str {
        self.into()
    }

    fn callback(self) -> SystemCallback {
        match self {
            Self::ResolveRenderTime => resolve_render_time,
            Self::SampleLocalPrediction => sample_local_prediction,
            Self::InterpolateRemoteSnapshots => interpolate_remote_snapshots,
            Self::SmoothReconciliationCorrections => smooth_reconciliation_corrections,
        }
    }

    pub(crate) fn register(
        self,
        world: &mut World,
        phase: PhaseId,
        driver_expression: &'static str,
        execution_context: FrameExecutionContext,
    ) -> Result<(), Error> {
        register_system(
            world,
            phase,
            driver_expression,
            PresentationPhase::SampleRenderTimeline,
            self.name(),
            execution_context,
            self.callback(),
        )
    }
}

/// A system in the client [`PresentationPhase::PrepareViewsAndListeners`] phase.
///
/// These systems update active views and spatial-audio listeners from sampled
/// scene state and the immutable configuration captured for the active frame.
#[derive(Debug, Clone, Copy, PartialEq, Eq, PartialOrd, Ord, Hash, IntoStaticStr)]
pub enum PrepareViewsAndListenersSystem {
    /// Associate active cameras, viewports, and spatial-audio listeners.
    SelectActiveViews,
    /// Resolve viewport dimensions, aspect ratios, and output regions.
    UpdateViewportLayouts,
    /// Update camera rigs from sampled scene state.
    UpdateCameraRigs,
    /// Derive projection parameters and view data for active cameras.
    UpdateCameraProjections,
}

impl PrepareViewsAndListenersSystem {
    /// Number of systems in `PrepareViewsAndListeners`.
    pub const COUNT: usize = 4;

    /// Stable registration order for `PrepareViewsAndListeners` systems.
    pub const ORDER: [Self; Self::COUNT] = [
        Self::SelectActiveViews,
        Self::UpdateViewportLayouts,
        Self::UpdateCameraRigs,
        Self::UpdateCameraProjections,
    ];

    /// Stable scheduler entity and trace field name for this system.
    #[must_use]
    pub fn name(self) -> &'static str {
        self.into()
    }

    fn callback(self) -> SystemCallback {
        match self {
            Self::SelectActiveViews => select_active_views,
            Self::UpdateViewportLayouts => update_viewport_layouts,
            Self::UpdateCameraRigs => update_camera_rigs,
            Self::UpdateCameraProjections => update_camera_projections,
        }
    }

    pub(crate) fn register(
        self,
        world: &mut World,
        phase: PhaseId,
        driver_expression: &'static str,
        execution_context: FrameExecutionContext,
    ) -> Result<(), Error> {
        register_system(
            world,
            phase,
            driver_expression,
            PresentationPhase::PrepareViewsAndListeners,
            self.name(),
            execution_context,
            self.callback(),
        )
    }
}

/// A system in the client [`PresentationPhase::EvaluateAnimationPoses`] phase.
///
/// These systems evaluate animation state into final model-space skeleton
/// poses and collect animation events for later presentation phases.
#[derive(Debug, Clone, Copy, PartialEq, Eq, PartialOrd, Ord, Hash, IntoStaticStr)]
pub enum EvaluateAnimationPosesSystem {
    /// Derive animation parameters from sampled scene state.
    UpdateAnimationParameters,
    /// Resolve animation states, transitions, times, layers, and weights.
    EvaluateAnimationGraphs,
    /// Sample the animation clips selected by the evaluated graphs.
    SampleAnimationClips,
    /// Blend base, partial, and additive animation layers.
    BlendAnimationLayers,
    /// Apply look-at, aim, recoil, and other procedural pose modifiers.
    ApplyProceduralPoseModifiers,
    /// Solve inverse-kinematics targets and constraints.
    SolveInverseKinematics,
    /// Convert the final local skeleton pose into model space.
    ConvertLocalPoseToModelSpace,
    /// Collect animation markers for later effects and feedback.
    CollectAnimationEvents,
}

impl EvaluateAnimationPosesSystem {
    /// Number of systems in `EvaluateAnimationPoses`.
    pub const COUNT: usize = 8;

    /// Stable registration order for `EvaluateAnimationPoses` systems.
    pub const ORDER: [Self; Self::COUNT] = [
        Self::UpdateAnimationParameters,
        Self::EvaluateAnimationGraphs,
        Self::SampleAnimationClips,
        Self::BlendAnimationLayers,
        Self::ApplyProceduralPoseModifiers,
        Self::SolveInverseKinematics,
        Self::ConvertLocalPoseToModelSpace,
        Self::CollectAnimationEvents,
    ];

    /// Stable scheduler entity and trace field name for this system.
    #[must_use]
    pub fn name(self) -> &'static str {
        self.into()
    }

    fn callback(self) -> SystemCallback {
        match self {
            Self::UpdateAnimationParameters => update_animation_parameters,
            Self::EvaluateAnimationGraphs => evaluate_animation_graphs,
            Self::SampleAnimationClips => sample_animation_clips,
            Self::BlendAnimationLayers => blend_animation_layers,
            Self::ApplyProceduralPoseModifiers => apply_procedural_pose_modifiers,
            Self::SolveInverseKinematics => solve_inverse_kinematics,
            Self::ConvertLocalPoseToModelSpace => convert_local_pose_to_model_space,
            Self::CollectAnimationEvents => collect_animation_events,
        }
    }

    pub(crate) fn register(
        self,
        world: &mut World,
        phase: PhaseId,
        driver_expression: &'static str,
        execution_context: FrameExecutionContext,
    ) -> Result<(), Error> {
        register_system(
            world,
            phase,
            driver_expression,
            PresentationPhase::EvaluateAnimationPoses,
            self.name(),
            execution_context,
            self.callback(),
        )
    }
}

/// A system in the client [`PresentationPhase::ResolveSceneGraph`] phase.
///
/// These systems resolve presentation-owned hierarchy dependencies and
/// transform animation, socket, and attachment state into final world-space
/// scene transforms.
#[derive(Debug, Clone, Copy, PartialEq, Eq, PartialOrd, Ord, Hash, IntoStaticStr)]
pub enum ResolveSceneGraphSystem {
    /// Resolve parent-child dependencies and reject invalid hierarchy cycles.
    ResolveSceneHierarchy,
    /// Publish evaluated model-space joint transforms into skeleton scene nodes.
    ApplySkeletonPoseTransforms,
    /// Resolve socket transforms from their joints, nodes, and configured offsets.
    ResolveSocketTransforms,
    /// Resolve attachment transforms from their parent or socket anchors.
    ResolveAttachmentTransforms,
    /// Compose final world-space transforms in stable hierarchy order.
    PropagateWorldTransforms,
    /// Resolve final camera world transforms after hierarchy and attachment propagation.
    ResolveCameraWorldTransforms,
    /// Resolve final spatial-audio listener poses and velocities.
    ResolveListenerWorldTransforms,
}

impl ResolveSceneGraphSystem {
    /// Number of systems in `ResolveSceneGraph`.
    pub const COUNT: usize = 7;

    /// Stable registration order for `ResolveSceneGraph` systems.
    pub const ORDER: [Self; Self::COUNT] = [
        Self::ResolveSceneHierarchy,
        Self::ApplySkeletonPoseTransforms,
        Self::ResolveSocketTransforms,
        Self::ResolveAttachmentTransforms,
        Self::PropagateWorldTransforms,
        Self::ResolveCameraWorldTransforms,
        Self::ResolveListenerWorldTransforms,
    ];

    /// Stable scheduler entity and trace field name for this system.
    #[must_use]
    pub fn name(self) -> &'static str {
        self.into()
    }

    fn callback(self) -> SystemCallback {
        match self {
            Self::ResolveSceneHierarchy => resolve_scene_hierarchy,
            Self::ApplySkeletonPoseTransforms => apply_skeleton_pose_transforms,
            Self::ResolveSocketTransforms => resolve_socket_transforms,
            Self::ResolveAttachmentTransforms => resolve_attachment_transforms,
            Self::PropagateWorldTransforms => propagate_world_transforms,
            Self::ResolveCameraWorldTransforms => resolve_camera_world_transforms,
            Self::ResolveListenerWorldTransforms => resolve_listener_world_transforms,
        }
    }

    pub(crate) fn register(
        self,
        world: &mut World,
        phase: PhaseId,
        driver_expression: &'static str,
        execution_context: FrameExecutionContext,
    ) -> Result<(), Error> {
        register_system(
            world,
            phase,
            driver_expression,
            PresentationPhase::ResolveSceneGraph,
            self.name(),
            execution_context,
            self.callback(),
        )
    }
}

/// A system in the client [`PresentationPhase::UpdateEffectsAndFeedback`] phase.
///
/// These systems resolve captured events into frame-local presentation cues
/// and advance presentation-owned visual, audio, interface, and haptic state.
#[derive(Debug, Clone, Copy, PartialEq, Eq, PartialOrd, Ord, Hash, IntoStaticStr)]
pub enum UpdateEffectsAndFeedbackSystem {
    /// Convert captured events and animation markers into frame-local cues.
    ResolvePresentationCues,
    /// Deduplicate and reconcile cues by stable authoritative or predicted event identity.
    DeduplicatePresentationCues,
    /// Create, advance, and retire visual effects.
    AdvanceVisualEffects,
    /// Update spatial-audio cues, emitters, parameters, and transforms.
    UpdateSpatialAudioEmitters,
    /// Update HUD, indicators, and world-space interface elements.
    UpdateUserInterface,
    /// Start, combine, advance, and retire haptic feedback envelopes.
    AdvanceHapticFeedback,
}

impl UpdateEffectsAndFeedbackSystem {
    /// Number of systems in `UpdateEffectsAndFeedback`.
    pub const COUNT: usize = 6;

    /// Stable registration order for `UpdateEffectsAndFeedback` systems.
    pub const ORDER: [Self; Self::COUNT] = [
        Self::ResolvePresentationCues,
        Self::DeduplicatePresentationCues,
        Self::AdvanceVisualEffects,
        Self::UpdateSpatialAudioEmitters,
        Self::UpdateUserInterface,
        Self::AdvanceHapticFeedback,
    ];

    /// Stable scheduler entity and trace field name for this system.
    #[must_use]
    pub fn name(self) -> &'static str {
        self.into()
    }

    fn callback(self) -> SystemCallback {
        match self {
            Self::ResolvePresentationCues => resolve_presentation_cues,
            Self::DeduplicatePresentationCues => deduplicate_presentation_cues,
            Self::AdvanceVisualEffects => advance_visual_effects,
            Self::UpdateSpatialAudioEmitters => update_spatial_audio_emitters,
            Self::UpdateUserInterface => update_user_interface,
            Self::AdvanceHapticFeedback => advance_haptic_feedback,
        }
    }

    pub(crate) fn register(
        self,
        world: &mut World,
        phase: PhaseId,
        driver_expression: &'static str,
        execution_context: FrameExecutionContext,
    ) -> Result<(), Error> {
        register_system(
            world,
            phase,
            driver_expression,
            PresentationPhase::UpdateEffectsAndFeedback,
            self.name(),
            execution_context,
            self.callback(),
        )
    }
}

/// A system in the client [`PresentationPhase::BuildFrameOutputs`] phase.
///
/// These systems transform finalized presentation state into immutable command
/// buffers without calling concrete client backends.
#[derive(Debug, Clone, Copy, PartialEq, Eq, PartialOrd, Ord, Hash, IntoStaticStr)]
pub enum BuildFrameOutputsSystem {
    /// Build semantic layer and view masks without performing renderer-owned culling.
    BuildSemanticVisibilityMasks,
    /// Build one complete immutable visual snapshot for the renderer.
    BuildRenderFrame,
    /// Build immutable listener, emitter, playback, parameter, and stop commands.
    BuildAudioCommands,
    /// Build immutable HUD and world-space interface commands.
    BuildUserInterfaceCommands,
    /// Build immutable device-targeted commands from active haptic envelopes.
    BuildHapticCommands,
    /// Validate and seal every frame output before publication.
    SealFrameOutputs,
}

impl BuildFrameOutputsSystem {
    /// Number of systems in `BuildFrameOutputs`.
    pub const COUNT: usize = 6;

    /// Stable registration order for `BuildFrameOutputs` systems.
    pub const ORDER: [Self; Self::COUNT] = [
        Self::BuildSemanticVisibilityMasks,
        Self::BuildRenderFrame,
        Self::BuildAudioCommands,
        Self::BuildUserInterfaceCommands,
        Self::BuildHapticCommands,
        Self::SealFrameOutputs,
    ];

    /// Stable scheduler entity and trace field name for this system.
    #[must_use]
    pub fn name(self) -> &'static str {
        self.into()
    }

    fn callback(self) -> SystemCallback {
        match self {
            Self::BuildSemanticVisibilityMasks => build_semantic_visibility_masks,
            Self::BuildRenderFrame => build_render_frame,
            Self::BuildAudioCommands => build_audio_commands,
            Self::BuildUserInterfaceCommands => build_user_interface_commands,
            Self::BuildHapticCommands => build_haptic_commands,
            Self::SealFrameOutputs => seal_frame_outputs,
        }
    }

    pub(crate) fn register(
        self,
        world: &mut World,
        phase: PhaseId,
        driver_expression: &'static str,
        execution_context: FrameExecutionContext,
    ) -> Result<(), Error> {
        register_system(
            world,
            phase,
            driver_expression,
            PresentationPhase::BuildFrameOutputs,
            self.name(),
            execution_context,
            self.callback(),
        )
    }
}

/// A system in the client [`PresentationPhase::PublishFrameOutputs`] phase.
///
/// These systems publish independently frame-keyed sealed outputs to bounded
/// backend handoffs. Swapchain acquisition and presentation remain renderer-owned.
#[derive(Debug, Clone, Copy, PartialEq, Eq, PartialOrd, Ord, Hash, IntoStaticStr)]
pub enum PublishFrameOutputsSystem {
    /// Publish the immutable visual snapshot to the latest-frame renderer mailbox.
    PublishRenderFrame,
    /// Publish sealed HUD and world-space interface commands.
    PublishUserInterfaceCommands,
    /// Publish sealed listener, emitter, playback, parameter, and stop commands.
    PublishAudioCommands,
    /// Publish sealed haptic commands to their target devices.
    PublishHapticCommands,
}

impl PublishFrameOutputsSystem {
    /// Number of systems in `PublishFrameOutputs`.
    pub const COUNT: usize = 4;

    /// Stable registration order for `PublishFrameOutputs` systems.
    pub const ORDER: [Self; Self::COUNT] = [
        Self::PublishRenderFrame,
        Self::PublishUserInterfaceCommands,
        Self::PublishAudioCommands,
        Self::PublishHapticCommands,
    ];

    /// Stable scheduler entity and trace field name for this system.
    #[must_use]
    pub fn name(self) -> &'static str {
        self.into()
    }

    fn callback(self) -> SystemCallback {
        match self {
            Self::PublishRenderFrame => publish_render_frame,
            Self::PublishUserInterfaceCommands => publish_user_interface_commands,
            Self::PublishAudioCommands => publish_audio_commands,
            Self::PublishHapticCommands => publish_haptic_commands,
        }
    }

    pub(crate) fn register(
        self,
        world: &mut World,
        phase: PhaseId,
        driver_expression: &'static str,
        execution_context: FrameExecutionContext,
    ) -> Result<(), Error> {
        register_system(
            world,
            phase,
            driver_expression,
            PresentationPhase::PublishFrameOutputs,
            self.name(),
            execution_context,
            self.callback(),
        )
    }
}

/// A system in the client [`PresentationPhase::CommitFrameHistory`] phase.
///
/// These systems commit presentation-owned temporal state and retire
/// frame-local inputs, events, and submitted commands after backend submission
/// succeeds. `PresentationWorld::frame` remains responsible for committing the
/// frame index after the complete pipeline returns successfully.
#[derive(Debug, Clone, Copy, PartialEq, Eq, PartialOrd, Ord, Hash, IntoStaticStr)]
pub enum CommitFrameHistorySystem {
    /// Commit final transforms for temporal rendering and visual velocities.
    CommitSceneTransformHistory,
    /// Commit evaluated animation states, times, ratios, and markers.
    CommitAnimationHistory,
    /// Commit previous camera and listener matrices, poses, and parameters.
    CommitCameraAndListenerHistory,
    /// Commit temporal visual, audio, interface, and haptic state.
    CommitEffectsAndFeedbackHistory,
    /// Retire simulation events and animation markers converted into cues.
    RetireConsumedEvents,
    /// Release frame-local captured prediction, snapshot, and configuration references.
    ReleaseCapturedFrameInputs,
    /// Release presentation-owned staging after backend ownership transfer.
    ReleasePublishedFrameOutputs,
}

impl CommitFrameHistorySystem {
    /// Number of systems in `CommitFrameHistory`.
    pub const COUNT: usize = 7;

    /// Stable registration order for `CommitFrameHistory` systems.
    pub const ORDER: [Self; Self::COUNT] = [
        Self::CommitSceneTransformHistory,
        Self::CommitAnimationHistory,
        Self::CommitCameraAndListenerHistory,
        Self::CommitEffectsAndFeedbackHistory,
        Self::RetireConsumedEvents,
        Self::ReleaseCapturedFrameInputs,
        Self::ReleasePublishedFrameOutputs,
    ];

    /// Stable scheduler entity and trace field name for this system.
    #[must_use]
    pub fn name(self) -> &'static str {
        self.into()
    }

    fn callback(self) -> SystemCallback {
        match self {
            Self::CommitSceneTransformHistory => commit_scene_transform_history,
            Self::CommitAnimationHistory => commit_animation_history,
            Self::CommitCameraAndListenerHistory => commit_camera_and_listener_history,
            Self::CommitEffectsAndFeedbackHistory => commit_effects_and_feedback_history,
            Self::RetireConsumedEvents => retire_consumed_events,
            Self::ReleaseCapturedFrameInputs => release_captured_frame_inputs,
            Self::ReleasePublishedFrameOutputs => release_published_frame_outputs,
        }
    }

    pub(crate) fn register(
        self,
        world: &mut World,
        phase: PhaseId,
        driver_expression: &'static str,
        execution_context: FrameExecutionContext,
    ) -> Result<(), Error> {
        register_system(
            world,
            phase,
            driver_expression,
            PresentationPhase::CommitFrameHistory,
            self.name(),
            execution_context,
            self.callback(),
        )
    }
}

pub(crate) fn register(
    world: &mut World,
    pipeline: PresentationPipeline,
    execution_context: FrameExecutionContext,
) -> Result<(), Error> {
    let driver_expression = register_system_driver(world)?;
    let phase = pipeline.phase(PresentationPhase::PrepareFrame);
    for system in PrepareFrameSystem::ORDER {
        system.register(world, phase, driver_expression, execution_context.clone())?;
    }

    let phase = pipeline.phase(PresentationPhase::CaptureFrameInputs);
    for system in CaptureFrameInputsSystem::ORDER {
        system.register(world, phase, driver_expression, execution_context.clone())?;
    }

    let phase = pipeline.phase(PresentationPhase::UpdateSceneProxies);
    for system in UpdateSceneProxiesSystem::ORDER {
        system.register(world, phase, driver_expression, execution_context.clone())?;
    }

    let phase = pipeline.phase(PresentationPhase::SampleRenderTimeline);
    for system in SampleRenderTimelineSystem::ORDER {
        system.register(world, phase, driver_expression, execution_context.clone())?;
    }

    let phase = pipeline.phase(PresentationPhase::PrepareViewsAndListeners);
    for system in PrepareViewsAndListenersSystem::ORDER {
        system.register(world, phase, driver_expression, execution_context.clone())?;
    }

    let phase = pipeline.phase(PresentationPhase::EvaluateAnimationPoses);
    for system in EvaluateAnimationPosesSystem::ORDER {
        system.register(world, phase, driver_expression, execution_context.clone())?;
    }

    let phase = pipeline.phase(PresentationPhase::ResolveSceneGraph);
    for system in ResolveSceneGraphSystem::ORDER {
        system.register(world, phase, driver_expression, execution_context.clone())?;
    }

    let phase = pipeline.phase(PresentationPhase::UpdateEffectsAndFeedback);
    for system in UpdateEffectsAndFeedbackSystem::ORDER {
        system.register(world, phase, driver_expression, execution_context.clone())?;
    }

    let phase = pipeline.phase(PresentationPhase::BuildFrameOutputs);
    for system in BuildFrameOutputsSystem::ORDER {
        system.register(world, phase, driver_expression, execution_context.clone())?;
    }

    let phase = pipeline.phase(PresentationPhase::PublishFrameOutputs);
    for system in PublishFrameOutputsSystem::ORDER {
        system.register(world, phase, driver_expression, execution_context.clone())?;
    }

    let phase = pipeline.phase(PresentationPhase::CommitFrameHistory);
    for system in CommitFrameHistorySystem::ORDER {
        system.register(world, phase, driver_expression, execution_context.clone())?;
    }
    Ok(())
}

fn register_system_driver(world: &mut World) -> Result<&'static str, Error> {
    let driver = world.register_tag::<PresentationSystemDriver>()?;
    let driver_entity = world.spawn()?;
    world.add_tag(driver_entity, driver)?;
    Ok(<PresentationSystemDriver as Tag>::NAME)
}

fn open_frame(execution_context: &FrameExecutionContext) -> SystemResult {
    // Open the frame-local working context prepared by PresentationWorld::frame.
    let _execution = execution_context.current();
    Ok(())
}

fn reset_frame_transient_storage(execution_context: &FrameExecutionContext) -> SystemResult {
    execution_context.reset_frame_transient()?;
    Ok(())
}

fn resolve_frame_timing(_execution_context: &FrameExecutionContext) -> SystemResult {
    // Apply the caller-configured pause, maximum-delta, and discontinuity policy.
    Ok(())
}

fn capture_client_view(_execution_context: &FrameExecutionContext) -> SystemResult {
    // Capture one immutable ClientView containing session and predicted state.
    Ok(())
}

fn capture_interpolation_window(_execution_context: &FrameExecutionContext) -> SystemResult {
    // Capture ClientView::authoritative_window without owning network history.
    Ok(())
}

fn capture_client_events(_execution_context: &FrameExecutionContext) -> SystemResult {
    // Capture available session, simulation, prediction, command, and voice events.
    Ok(())
}

fn capture_frame_configuration(_execution_context: &FrameExecutionContext) -> SystemResult {
    // Capture the active view and output configuration for this frame.
    Ok(())
}

fn capture_backend_feedback(_execution_context: &FrameExecutionContext) -> SystemResult {
    // Capture immutable resource residency, device, and submission feedback.
    Ok(())
}

fn create_missing_scene_proxies(_execution_context: &FrameExecutionContext) -> SystemResult {
    // Create presentation-owned proxies for captured sources that lack one.
    Ok(())
}

fn refresh_scene_proxy_bindings(_execution_context: &FrameExecutionContext) -> SystemResult {
    // Refresh source identity, resource descriptors, capabilities, and metadata.
    Ok(())
}

fn retire_stale_scene_proxies(_execution_context: &FrameExecutionContext) -> SystemResult {
    // Retire proxies whose captured sources disappeared or expired.
    Ok(())
}

fn resolve_render_time(_execution_context: &FrameExecutionContext) -> SystemResult {
    // Resolve the sample time from frame timing and interpolation configuration.
    Ok(())
}

fn sample_local_prediction(_execution_context: &FrameExecutionContext) -> SystemResult {
    // Sample presentation state for locally predicted entities.
    Ok(())
}

fn interpolate_remote_snapshots(_execution_context: &FrameExecutionContext) -> SystemResult {
    // Interpolate remote entity state from captured snapshot history.
    Ok(())
}

fn smooth_reconciliation_corrections(_execution_context: &FrameExecutionContext) -> SystemResult {
    // Smooth visual corrections without mutating reconciled prediction state.
    Ok(())
}

fn select_active_views(_execution_context: &FrameExecutionContext) -> SystemResult {
    // Associate active cameras, viewports, and spatial-audio listeners.
    Ok(())
}

fn update_viewport_layouts(_execution_context: &FrameExecutionContext) -> SystemResult {
    // Resolve viewport dimensions, aspect ratios, and output regions.
    Ok(())
}

fn update_camera_rigs(_execution_context: &FrameExecutionContext) -> SystemResult {
    // Update camera rigs from sampled scene state.
    Ok(())
}

fn update_camera_projections(_execution_context: &FrameExecutionContext) -> SystemResult {
    // Derive projection parameters and view data for active cameras.
    Ok(())
}

fn update_animation_parameters(_execution_context: &FrameExecutionContext) -> SystemResult {
    // Derive animation parameters from sampled scene state.
    Ok(())
}

fn evaluate_animation_graphs(_execution_context: &FrameExecutionContext) -> SystemResult {
    // Resolve animation states, transitions, times, layers, and weights.
    Ok(())
}

fn sample_animation_clips(_execution_context: &FrameExecutionContext) -> SystemResult {
    // Sample the animation clips selected by the evaluated graphs.
    Ok(())
}

fn blend_animation_layers(_execution_context: &FrameExecutionContext) -> SystemResult {
    // Blend base, partial, and additive animation layers.
    Ok(())
}

fn apply_procedural_pose_modifiers(_execution_context: &FrameExecutionContext) -> SystemResult {
    // Apply look-at, aim, recoil, and other procedural pose modifiers.
    Ok(())
}

fn solve_inverse_kinematics(_execution_context: &FrameExecutionContext) -> SystemResult {
    // Solve inverse-kinematics targets and constraints.
    Ok(())
}

fn convert_local_pose_to_model_space(_execution_context: &FrameExecutionContext) -> SystemResult {
    // Convert the final local skeleton pose into model space.
    Ok(())
}

fn collect_animation_events(_execution_context: &FrameExecutionContext) -> SystemResult {
    // Collect animation markers for later effects and feedback.
    Ok(())
}

fn resolve_scene_hierarchy(_execution_context: &FrameExecutionContext) -> SystemResult {
    // Resolve parent-child dependencies and reject invalid hierarchy cycles.
    Ok(())
}

fn apply_skeleton_pose_transforms(_execution_context: &FrameExecutionContext) -> SystemResult {
    // Publish evaluated model-space joint transforms into skeleton scene nodes.
    Ok(())
}

fn resolve_socket_transforms(_execution_context: &FrameExecutionContext) -> SystemResult {
    // Resolve socket transforms from their joints, nodes, and configured offsets.
    Ok(())
}

fn resolve_attachment_transforms(_execution_context: &FrameExecutionContext) -> SystemResult {
    // Resolve attachment transforms from their parent or socket anchors.
    Ok(())
}

fn propagate_world_transforms(_execution_context: &FrameExecutionContext) -> SystemResult {
    // Compose final world-space transforms in stable hierarchy order.
    Ok(())
}

fn resolve_camera_world_transforms(_execution_context: &FrameExecutionContext) -> SystemResult {
    // Resolve final camera transforms after hierarchy, skeletons, sockets, and attachments.
    Ok(())
}

fn resolve_listener_world_transforms(_execution_context: &FrameExecutionContext) -> SystemResult {
    // Resolve final listener poses and velocities from the completed scene graph.
    Ok(())
}

fn resolve_presentation_cues(_execution_context: &FrameExecutionContext) -> SystemResult {
    // Convert captured events and animation markers into frame-local cues.
    Ok(())
}

fn deduplicate_presentation_cues(_execution_context: &FrameExecutionContext) -> SystemResult {
    // Reconcile authoritative and predicted cue identities before advancing effects.
    Ok(())
}

fn advance_visual_effects(_execution_context: &FrameExecutionContext) -> SystemResult {
    // Create, advance, and retire presentation-owned visual effects.
    Ok(())
}

fn update_spatial_audio_emitters(execution_context: &FrameExecutionContext) -> SystemResult {
    execution_context.update_audio_emitters()?;
    Ok(())
}

fn update_user_interface(_execution_context: &FrameExecutionContext) -> SystemResult {
    // Update HUD, indicators, and world-space interface elements.
    Ok(())
}

fn advance_haptic_feedback(_execution_context: &FrameExecutionContext) -> SystemResult {
    // Start, combine, advance, and retire haptic feedback envelopes.
    Ok(())
}

fn build_semantic_visibility_masks(_execution_context: &FrameExecutionContext) -> SystemResult {
    // Build semantic view masks; the renderer owns culling and LOD selection.
    Ok(())
}

fn build_render_frame(execution_context: &FrameExecutionContext) -> SystemResult {
    execution_context.build_render_frame()?;
    Ok(())
}

fn build_audio_commands(execution_context: &FrameExecutionContext) -> SystemResult {
    execution_context.build_audio_commands()?;
    Ok(())
}

fn build_user_interface_commands(_execution_context: &FrameExecutionContext) -> SystemResult {
    // Build immutable HUD and world-space interface commands.
    Ok(())
}

fn build_haptic_commands(_execution_context: &FrameExecutionContext) -> SystemResult {
    // Build immutable device-targeted commands from active haptic envelopes.
    Ok(())
}

fn seal_frame_outputs(_execution_context: &FrameExecutionContext) -> SystemResult {
    // Validate complete immutable outputs and their frame-keyed identities.
    Ok(())
}

fn publish_render_frame(execution_context: &FrameExecutionContext) -> SystemResult {
    execution_context.publish_render_frame()?;
    Ok(())
}

fn publish_user_interface_commands(_execution_context: &FrameExecutionContext) -> SystemResult {
    // Publish a frame-keyed sealed HUD and world-space interface batch.
    Ok(())
}

fn publish_audio_commands(execution_context: &FrameExecutionContext) -> SystemResult {
    execution_context.publish_audio_commands()?;
    Ok(())
}

fn publish_haptic_commands(_execution_context: &FrameExecutionContext) -> SystemResult {
    // Publish frame-keyed sealed haptic commands to their device backend.
    Ok(())
}

fn commit_scene_transform_history(_execution_context: &FrameExecutionContext) -> SystemResult {
    // Commit final transforms for temporal rendering and visual velocities.
    Ok(())
}

fn commit_animation_history(_execution_context: &FrameExecutionContext) -> SystemResult {
    // Commit evaluated animation states, times, ratios, and markers.
    Ok(())
}

fn commit_camera_and_listener_history(_execution_context: &FrameExecutionContext) -> SystemResult {
    // Commit previous camera and listener matrices, poses, and parameters.
    Ok(())
}

fn commit_effects_and_feedback_history(_execution_context: &FrameExecutionContext) -> SystemResult {
    // Commit temporal visual, audio, interface, and haptic state.
    Ok(())
}

fn retire_consumed_events(_execution_context: &FrameExecutionContext) -> SystemResult {
    // Retire simulation events and animation markers converted into cues.
    Ok(())
}

fn release_captured_frame_inputs(_execution_context: &FrameExecutionContext) -> SystemResult {
    // Release frame-local captured inputs without mutating their sources.
    Ok(())
}

fn release_published_frame_outputs(_execution_context: &FrameExecutionContext) -> SystemResult {
    // Release only presentation staging; backend-owned frames and resources remain alive.
    Ok(())
}

fn register_system<F>(
    world: &mut World,
    phase: PhaseId,
    driver_expression: &'static str,
    presentation_phase: PresentationPhase,
    system_name: &'static str,
    execution_context: FrameExecutionContext,
    callback: F,
) -> Result<(), Error>
where
    F: Fn(&FrameExecutionContext) -> SystemResult + 'static,
{
    world
        .system(system_name, driver_expression)?
        .phase(phase)?
        .project(())?
        .each(move |_context, _entity, ()| {
            callback(&execution_context)?;
            telemetry::system_executed(
                presentation_phase,
                system_name,
                execution_context.current(),
            );
            Ok(())
        })?;
    Ok(())
}
