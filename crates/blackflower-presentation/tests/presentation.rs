use std::error::Error as StdError;
use std::io;
use std::sync::atomic::{AtomicBool, Ordering};
use std::sync::{Arc, Mutex};

use blackflower_ecs::{Component, Read, TickDelta, World};
use blackflower_presentation::{
    BuildBackendCommandsSystem, CaptureFrameInputsSystem, CommitFrameHistorySystem,
    EvaluateAnimationPosesSystem, FrameExecution, FrameIndex, PrepareFrameSystem,
    PresentationError, PresentationPhase, PresentationPipeline, PresentationWorld,
    ResolveSceneGraphSystem, SampleRenderTimelineSystem, SubmitBackendCommandsSystem,
    UpdateCamerasAndListenersSystem, UpdateEffectsAndFeedbackSystem, UpdateSceneProxiesSystem,
};
use bytemuck::{Pod, Zeroable};

type TestResult = Result<(), Box<dyn StdError>>;

#[derive(Clone, Copy, Pod, Zeroable, Component)]
#[repr(transparent)]
struct Probe(u8);

#[test]
fn phase_names_are_stable() {
    assert_eq!(
        PresentationPhase::ORDER.map(PresentationPhase::name),
        [
            "PrepareFrame",
            "CaptureFrameInputs",
            "UpdateSceneProxies",
            "SampleRenderTimeline",
            "UpdateCamerasAndListeners",
            "EvaluateAnimationPoses",
            "ResolveSceneGraph",
            "UpdateEffectsAndFeedback",
            "BuildBackendCommands",
            "SubmitBackendCommands",
            "CommitFrameHistory",
        ]
    );
}

#[test]
fn prepare_frame_system_names_are_stable() {
    assert_eq!(
        PrepareFrameSystem::ORDER.map(PrepareFrameSystem::name),
        ["OpenFrame", "ResetFrameTransientStorage"]
    );
}

#[test]
fn capture_frame_inputs_system_names_are_stable() {
    assert_eq!(
        CaptureFrameInputsSystem::ORDER.map(CaptureFrameInputsSystem::name),
        [
            "CaptureLocalPredictionState",
            "CaptureRemoteSnapshotHistory",
            "CaptureSimulationEvents",
            "CaptureFrameConfiguration",
        ]
    );
}

#[test]
fn update_scene_proxies_system_names_are_stable() {
    assert_eq!(
        UpdateSceneProxiesSystem::ORDER.map(UpdateSceneProxiesSystem::name),
        [
            "CreateMissingSceneProxies",
            "RefreshSceneProxyBindings",
            "RetireStaleSceneProxies",
        ]
    );
}

#[test]
fn sample_render_timeline_system_names_are_stable() {
    assert_eq!(
        SampleRenderTimelineSystem::ORDER.map(SampleRenderTimelineSystem::name),
        [
            "ResolveRenderTime",
            "SampleLocalPrediction",
            "InterpolateRemoteSnapshots",
            "SmoothReconciliationCorrections",
        ]
    );
}

#[test]
fn update_cameras_and_listeners_system_names_are_stable() {
    assert_eq!(
        UpdateCamerasAndListenersSystem::ORDER.map(UpdateCamerasAndListenersSystem::name),
        [
            "SelectActiveViews",
            "UpdateViewportLayouts",
            "UpdateCameraRigs",
            "UpdateCameraProjections",
            "UpdateSpatialAudioListeners",
        ]
    );
}

#[test]
fn evaluate_animation_poses_system_names_are_stable() {
    assert_eq!(
        EvaluateAnimationPosesSystem::ORDER.map(EvaluateAnimationPosesSystem::name),
        [
            "UpdateAnimationParameters",
            "EvaluateAnimationGraphs",
            "SampleAnimationClips",
            "BlendAnimationLayers",
            "ApplyProceduralPoseModifiers",
            "SolveInverseKinematics",
            "ConvertLocalPoseToModelSpace",
            "CollectAnimationEvents",
        ]
    );
}

#[test]
fn resolve_scene_graph_system_names_are_stable() {
    assert_eq!(
        ResolveSceneGraphSystem::ORDER.map(ResolveSceneGraphSystem::name),
        [
            "ResolveSceneHierarchy",
            "ApplySkeletonPoseTransforms",
            "ResolveSocketTransforms",
            "ResolveAttachmentTransforms",
            "PropagateWorldTransforms",
        ]
    );
}

#[test]
fn update_effects_and_feedback_system_names_are_stable() {
    assert_eq!(
        UpdateEffectsAndFeedbackSystem::ORDER.map(UpdateEffectsAndFeedbackSystem::name),
        [
            "ResolvePresentationCues",
            "AdvanceVisualEffects",
            "UpdateSpatialAudioEmitters",
            "UpdateUserInterface",
            "AdvanceHapticFeedback",
        ]
    );
}

#[test]
fn build_backend_commands_system_names_are_stable() {
    assert_eq!(
        BuildBackendCommandsSystem::ORDER.map(BuildBackendCommandsSystem::name),
        [
            "BuildVisibilitySets",
            "BuildRenderingCommands",
            "BuildAudioCommands",
            "BuildUserInterfaceCommands",
            "BuildHapticCommands",
            "SealBackendCommands",
        ]
    );
}

#[test]
fn submit_backend_commands_system_names_are_stable() {
    assert_eq!(
        SubmitBackendCommandsSystem::ORDER.map(SubmitBackendCommandsSystem::name),
        [
            "SubmitRenderingCommands",
            "SubmitUserInterfaceCommands",
            "SubmitAudioCommands",
            "SubmitHapticCommands",
            "PresentViewportFrames",
        ]
    );
}

#[test]
fn commit_frame_history_system_names_are_stable() {
    assert_eq!(
        CommitFrameHistorySystem::ORDER.map(CommitFrameHistorySystem::name),
        [
            "CommitSceneTransformHistory",
            "CommitAnimationHistory",
            "CommitCameraAndListenerHistory",
            "CommitEffectsAndFeedbackHistory",
            "RetireConsumedEvents",
            "ReleaseCapturedFrameInputs",
            "RecycleSubmittedBackendCommands",
        ]
    );
}

#[test]
fn pipeline_orders_systems_by_phase_instead_of_registration_order() -> TestResult {
    let mut world = World::new()?;
    let probe = world.register_component::<Probe>()?;
    let entity = world.spawn()?;
    world.insert(entity, probe, Probe(0))?;

    let pipeline = PresentationPipeline::register(&mut world)?;
    let observed = Arc::new(Mutex::new(Vec::new()));

    for phase in PresentationPhase::ORDER.into_iter().rev() {
        let system_name = format!("Record{}", phase.name());
        let observed_by_system = Arc::clone(&observed);
        world
            .system(&system_name, "Probe")?
            .phase(pipeline.phase(phase))?
            .project(Read::<Probe>::field(0))?
            .each(move |_context, _entity, _probe| {
                let mut observed = observed_by_system
                    .lock()
                    .map_err(|_error| io::Error::other("phase order lock poisoned"))?;
                observed.push(phase);
                Ok(())
            })?;
    }

    assert!(world.progress(TickDelta::from_seconds(1.0 / 60.0)?)?);

    let observed = observed
        .lock()
        .map_err(|_error| io::Error::other("phase order lock poisoned"))?;
    assert_eq!(observed.as_slice(), PresentationPhase::ORDER);
    Ok(())
}

#[test]
fn presentation_world_advances_with_the_supplied_frame_delta() -> TestResult {
    let mut presentation = PresentationWorld::new()?;
    let probe = presentation.ecs_mut().register_component::<Probe>()?;
    let entity = presentation.ecs_mut().spawn()?;
    presentation.ecs_mut().insert(entity, probe, Probe(0))?;

    let execution_context = presentation.execution_context();
    let observed = Arc::new(Mutex::new(Vec::new()));
    let observed_by_system = Arc::clone(&observed);
    let prepare_frame = presentation.phase(PresentationPhase::PrepareFrame);
    presentation
        .ecs_mut()
        .system("RecordFrameExecution", "Probe")?
        .phase(prepare_frame)?
        .project(Read::<Probe>::field(0))?
        .each(move |context, _entity, _probe| {
            let mut observed = observed_by_system
                .lock()
                .map_err(|_error| io::Error::other("frame execution lock poisoned"))?;
            observed.push((
                execution_context.current(),
                context.delta().as_seconds().to_bits(),
            ));
            Ok(())
        })?;

    let first_delta = TickDelta::from_seconds(1.0 / 60.0)?;
    let second_delta = TickDelta::from_seconds(1.0 / 144.0)?;
    assert!(presentation.frame(first_delta)?);
    assert!(presentation.frame(second_delta)?);

    assert_eq!(presentation.current_frame(), FrameIndex::new(2));
    assert_eq!(
        observed
            .lock()
            .map_err(|_error| io::Error::other("frame execution lock poisoned"))?
            .as_slice(),
        [
            (
                FrameExecution {
                    frame: FrameIndex::new(1),
                },
                first_delta.as_seconds().to_bits(),
            ),
            (
                FrameExecution {
                    frame: FrameIndex::new(2),
                },
                second_delta.as_seconds().to_bits(),
            ),
        ]
    );
    Ok(())
}

#[test]
fn failed_submission_does_not_commit_frame_history() -> TestResult {
    let mut presentation = PresentationWorld::new()?;
    let probe = presentation.ecs_mut().register_component::<Probe>()?;
    let entity = presentation.ecs_mut().spawn()?;
    presentation.ecs_mut().insert(entity, probe, Probe(0))?;

    let submit_commands = presentation.phase(PresentationPhase::SubmitBackendCommands);
    presentation
        .ecs_mut()
        .system("FailBackendSubmission", "Probe")?
        .phase(submit_commands)?
        .project(Read::<Probe>::field(0))?
        .each(|_context, _entity, _probe| {
            Err(io::Error::other("expected backend submission failure").into())
        })?;

    let history_committed = Arc::new(AtomicBool::new(false));
    let history_committed_by_system = Arc::clone(&history_committed);
    let commit_history = presentation.phase(PresentationPhase::CommitFrameHistory);
    presentation
        .ecs_mut()
        .system("RecordFrameHistoryCommit", "Probe")?
        .phase(commit_history)?
        .project(Read::<Probe>::field(0))?
        .each(move |_context, _entity, _probe| {
            history_committed_by_system.store(true, Ordering::Release);
            Ok(())
        })?;

    let Err(error) = presentation.frame(TickDelta::from_seconds(1.0 / 60.0)?) else {
        return Err(io::Error::other("backend submission must fail").into());
    };
    assert!(matches!(error, PresentationError::Run(_)));
    assert_eq!(
        error.to_string(),
        "system \"FailBackendSubmission\" failed: expected backend submission failure"
    );
    assert_eq!(presentation.current_frame(), FrameIndex::ZERO);
    assert_eq!(
        presentation.execution_context().current(),
        FrameExecution {
            frame: FrameIndex::ZERO,
        }
    );
    assert!(!history_committed.load(Ordering::Acquire));
    Ok(())
}
