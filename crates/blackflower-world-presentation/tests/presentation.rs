use std::error::Error as StdError;
use std::io;
use std::sync::atomic::{AtomicBool, Ordering};
use std::sync::{Arc, Mutex};

use blackflower_acoustics::{
    AcousticStructureVersion, AudibleSoundDelivery, BandEnergy, PropagationDescriptor,
};
use blackflower_ecs::{Component, Read, TickDelta, World};
use blackflower_rendering::{RenderFrameId, ResourceHandle};
use blackflower_world_presentation::{
    AudioCommand, BuildFrameOutputsSystem, CaptureFrameInputsSystem, CommitFrameHistorySystem,
    EvaluateAnimationPosesSystem, FrameExecution, FrameIndex, LocalVisualBinding, MovementProxy,
    MovementSampleKind, MovementSourceId, PrepareFrameSystem, PrepareViewsAndListenersSystem,
    PresentationError, PresentationMovementSample, PresentationPhase, PresentationPipeline,
    PresentationViewport, PresentationWorld, PublishFrameOutputsSystem, ResolveSceneGraphSystem,
    SampleRenderTimelineSystem, UpdateEffectsAndFeedbackSystem, UpdateSceneProxiesSystem,
};
use bytemuck::{Pod, Zeroable};

type TestResult = Result<(), Box<dyn StdError>>;

fn assert_f64_array_close<const N: usize>(actual: [f64; N], expected: [f64; N]) {
    assert!(
        actual
            .into_iter()
            .zip(expected)
            .all(|(actual, expected)| (actual - expected).abs() <= 1.0e-12),
        "expected {expected:?}, got {actual:?}"
    );
}

fn movement_sample(
    source: MovementSourceId,
    position_meters: [f64; 3],
    orientation: [f64; 4],
    kind: MovementSampleKind,
) -> Result<PresentationMovementSample, Box<dyn StdError>> {
    Ok(PresentationMovementSample::new(
        source,
        position_meters,
        orientation,
        kind,
    )?)
}

fn local_movement_proxy(
    presentation: &PresentationWorld,
) -> Result<MovementProxy, Box<dyn StdError>> {
    Ok(presentation
        .local_movement_proxy()?
        .ok_or_else(|| io::Error::other("local movement proxy is missing"))?)
}

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
            "PrepareViewsAndListeners",
            "EvaluateAnimationPoses",
            "ResolveSceneGraph",
            "UpdateEffectsAndFeedback",
            "BuildFrameOutputs",
            "PublishFrameOutputs",
            "CommitFrameHistory",
        ]
    );
}

#[test]
fn prepare_frame_system_names_are_stable() {
    assert_eq!(
        PrepareFrameSystem::ORDER.map(PrepareFrameSystem::name),
        [
            "OpenFrame",
            "ResetFrameTransientStorage",
            "ResolveFrameTiming",
        ]
    );
}

#[test]
fn capture_frame_inputs_system_names_are_stable() {
    assert_eq!(
        CaptureFrameInputsSystem::ORDER.map(CaptureFrameInputsSystem::name),
        [
            "CaptureClientView",
            "CaptureInterpolationWindow",
            "CaptureClientEvents",
            "CaptureFrameConfiguration",
            "CaptureBackendFeedback",
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
fn prepare_views_and_listeners_system_names_are_stable() {
    assert_eq!(
        PrepareViewsAndListenersSystem::ORDER.map(PrepareViewsAndListenersSystem::name),
        [
            "SelectActiveViews",
            "UpdateViewportLayouts",
            "UpdateCameraRigs",
            "UpdateCameraProjections",
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
            "ResolveCameraWorldTransforms",
            "ResolveListenerWorldTransforms",
        ]
    );
}

#[test]
fn update_effects_and_feedback_system_names_are_stable() {
    assert_eq!(
        UpdateEffectsAndFeedbackSystem::ORDER.map(UpdateEffectsAndFeedbackSystem::name),
        [
            "ResolvePresentationCues",
            "DeduplicatePresentationCues",
            "AdvanceVisualEffects",
            "UpdateSpatialAudioEmitters",
            "UpdateUserInterface",
            "AdvanceHapticFeedback",
        ]
    );
}

#[test]
fn build_frame_outputs_system_names_are_stable() {
    assert_eq!(
        BuildFrameOutputsSystem::ORDER.map(BuildFrameOutputsSystem::name),
        [
            "BuildSemanticVisibilityMasks",
            "BuildRenderFrame",
            "BuildAudioCommands",
            "BuildUserInterfaceCommands",
            "BuildHapticCommands",
            "SealFrameOutputs",
        ]
    );
}

#[test]
fn publish_frame_outputs_system_names_are_stable() {
    assert_eq!(
        PublishFrameOutputsSystem::ORDER.map(PublishFrameOutputsSystem::name),
        [
            "PublishRenderFrame",
            "PublishUserInterfaceCommands",
            "PublishAudioCommands",
            "PublishHapticCommands",
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
            "ReleasePublishedFrameOutputs",
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

    let submit_commands = presentation.phase(PresentationPhase::PublishFrameOutputs);
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

#[test]
fn local_movement_proxy_tracks_prediction_and_retires_missing_sources() -> TestResult {
    let mut presentation = PresentationWorld::new()?;
    let source = MovementSourceId::new(41)?;
    let delta = TickDelta::from_seconds(0.025)?;

    presentation.set_local_movement_sample(Some(movement_sample(
        source,
        [0.0, 1.0, 2.0],
        [0.0, 0.0, 0.0, 1.0],
        MovementSampleKind::Predicted,
    )?))?;
    assert!(presentation.frame(delta)?);
    let initial = local_movement_proxy(&presentation)?;
    assert_eq!(initial.source(), source);
    assert_f64_array_close(initial.predicted_position_meters(), [0.0, 1.0, 2.0]);
    assert_f64_array_close(initial.visual_position_meters(), [0.0, 1.0, 2.0]);
    assert!(!initial.correction_active());

    presentation.set_local_movement_sample(Some(movement_sample(
        source,
        [12.0, 1.0, 2.0],
        [0.0, 0.0, 0.0, 1.0],
        MovementSampleKind::Predicted,
    )?))?;
    assert!(presentation.frame(delta)?);
    let advanced = local_movement_proxy(&presentation)?;
    assert_f64_array_close(advanced.visual_position_meters(), [12.0, 1.0, 2.0]);
    assert!(!advanced.correction_active());

    presentation.set_local_movement_sample(None)?;
    assert!(presentation.frame(delta)?);
    assert_eq!(presentation.local_movement_proxy()?, None);
    Ok(())
}

#[test]
fn local_movement_proxy_smooths_reconciliation_without_prediction_latency() -> TestResult {
    let mut presentation = PresentationWorld::new()?;
    let source = MovementSourceId::new(41)?;
    let delta = TickDelta::from_seconds(0.025)?;
    let initial = movement_sample(
        source,
        [0.0, 1.0, 2.0],
        [0.0, 0.0, 0.0, 1.0],
        MovementSampleKind::Predicted,
    )?;
    presentation.set_local_movement_sample(Some(initial))?;
    assert!(presentation.frame(delta)?);

    let corrected = movement_sample(
        source,
        [10.0, 1.0, 2.0],
        [0.0, 1.0, 0.0, 0.0],
        MovementSampleKind::Reconciled,
    )?;
    presentation.set_local_movement_sample(Some(corrected))?;
    assert!(presentation.frame(delta)?);
    let correcting = local_movement_proxy(&presentation)?;
    assert_f64_array_close(correcting.predicted_position_meters(), [10.0, 1.0, 2.0]);
    assert_f64_array_close(correcting.visual_position_meters(), [2.5, 1.0, 2.0]);
    assert!(correcting.correction_active());
    assert!(
        correcting
            .visual_orientation()
            .into_iter()
            .zip(correcting.predicted_orientation())
            .any(|(visual, predicted)| (visual - predicted).abs() > 1.0e-12)
    );

    let predicted = movement_sample(
        source,
        [10.0, 1.0, 2.0],
        [0.0, 1.0, 0.0, 0.0],
        MovementSampleKind::Predicted,
    )?;
    presentation.set_local_movement_sample(Some(predicted))?;
    for _frame in 0..3 {
        assert!(presentation.frame(delta)?);
    }
    let settled = local_movement_proxy(&presentation)?;
    assert_f64_array_close(settled.visual_position_meters(), [10.0, 1.0, 2.0]);
    assert_f64_array_close(
        settled.visual_orientation(),
        settled.predicted_orientation(),
    );
    assert!(!settled.correction_active());
    Ok(())
}

#[test]
fn failed_frame_does_not_commit_local_movement_proxy() -> TestResult {
    let mut presentation = PresentationWorld::new()?;
    let source = MovementSourceId::new(7)?;
    let sample = |position| {
        movement_sample(
            source,
            [position, 0.0, 0.0],
            [0.0, 0.0, 0.0, 1.0],
            MovementSampleKind::Predicted,
        )
    };
    presentation.set_local_movement_sample(Some(sample(1.0)?))?;
    assert!(presentation.frame(TickDelta::from_seconds(1.0 / 60.0)?)?);

    let probe = presentation.ecs_mut().register_component::<Probe>()?;
    let entity = presentation.ecs_mut().spawn()?;
    presentation.ecs_mut().insert(entity, probe, Probe(0))?;
    let publish_outputs = presentation.phase(PresentationPhase::PublishFrameOutputs);
    presentation
        .ecs_mut()
        .system("FailMovementProxyFrame", "Probe")?
        .phase(publish_outputs)?
        .project(Read::<Probe>::field(0))?
        .each(|_context, _entity, _probe| {
            Err(io::Error::other("expected movement frame failure").into())
        })?;

    presentation.set_local_movement_sample(Some(sample(2.0)?))?;
    assert!(
        presentation
            .frame(TickDelta::from_seconds(1.0 / 60.0)?)
            .is_err()
    );
    assert_f64_array_close(
        local_movement_proxy(&presentation)?.visual_position_meters(),
        [1.0, 0.0, 0.0],
    );
    Ok(())
}

#[test]
fn retrying_a_failed_frame_does_not_republish_backend_effects() -> TestResult {
    let mut presentation = PresentationWorld::new()?;
    let mailbox = presentation.render_mailbox();
    let probe = presentation.ecs_mut().register_component::<Probe>()?;
    let entity = presentation.ecs_mut().spawn()?;
    presentation.ecs_mut().insert(entity, probe, Probe(0))?;
    let delivery = audible_sound_delivery(91);
    presentation.queue_audible_sound(delivery)?;

    let fail_once = Arc::new(AtomicBool::new(true));
    let fail_once_by_system = Arc::clone(&fail_once);
    let publish_outputs = presentation.phase(PresentationPhase::PublishFrameOutputs);
    presentation
        .ecs_mut()
        .system("FailFirstBackendPublication", "Probe")?
        .phase(publish_outputs)?
        .project(Read::<Probe>::field(0))?
        .each(move |_context, _entity, _probe| {
            if fail_once_by_system.swap(false, Ordering::AcqRel) {
                Err(io::Error::other("expected first publication failure").into())
            } else {
                Ok(())
            }
        })?;

    assert!(
        presentation
            .frame(TickDelta::from_seconds(1.0 / 60.0)?)
            .is_err()
    );
    assert_eq!(presentation.current_frame(), FrameIndex::ZERO);
    assert_eq!(
        mailbox.take_latest()?.map(|frame| frame.id),
        Some(RenderFrameId::new(1))
    );
    assert_eq!(
        presentation.drain_submitted_audio_commands()?,
        [AudioCommand::PlayAudibleSound(delivery)]
    );

    assert!(presentation.frame(TickDelta::from_seconds(1.0 / 60.0)?)?);
    assert_eq!(presentation.current_frame(), FrameIndex::new(1));
    assert_eq!(mailbox.pending_id()?, None);
    assert!(presentation.drain_submitted_audio_commands()?.is_empty());
    Ok(())
}

#[test]
fn audible_delivery_flows_through_the_three_audio_systems() -> TestResult {
    let mut presentation = PresentationWorld::new()?;
    let delivery = audible_sound_delivery(77);
    presentation.queue_audible_sound(delivery)?;
    assert!(presentation.frame(TickDelta::from_seconds(1.0 / 60.0)?)?);
    assert_eq!(
        presentation.drain_submitted_audio_commands()?,
        vec![AudioCommand::PlayAudibleSound(delivery)]
    );
    Ok(())
}

fn audible_sound_delivery(client_event_id: u32) -> AudibleSoundDelivery {
    AudibleSoundDelivery {
        receiver_id: 20,
        client_event_id,
        play_sample: 4_800,
        propagation: PropagationDescriptor {
            structure_version: AcousticStructureVersion(3),
            arrival_sample: 4_800,
            path_length_mm: 34_300,
            gain_db_q8: -12 * 256,
            band_gain: BandEnergy([60_000, 40_000, 20_000]),
            direction_q15: [100, 200, 300],
            uncertainty_q16: 512,
            direct: false,
        },
    }
}

#[test]
fn presentation_publishes_one_complete_latest_render_frame() -> TestResult {
    let mut presentation = PresentationWorld::new()?;
    let mailbox = presentation.render_mailbox();

    assert!(presentation.frame(TickDelta::from_seconds(1.0 / 60.0)?)?);
    assert_eq!(mailbox.pending_id()?, Some(RenderFrameId::new(1)));
    assert_eq!(
        mailbox.take_latest()?.map(|frame| frame.id),
        Some(RenderFrameId::new(1))
    );
    Ok(())
}

#[test]
fn local_visual_binding_builds_instance_and_follow_camera() -> TestResult {
    let mut presentation = PresentationWorld::new()?;
    let resource = ResourceHandle::new(19);
    let source = MovementSourceId::new(41)?;
    presentation.set_local_visual_binding(Some(LocalVisualBinding::new(resource)))?;
    presentation.set_viewport(Some(PresentationViewport::new(1280, 720)?))?;
    presentation.set_local_movement_sample(Some(movement_sample(
        source,
        [3.0, 4.0, 5.0],
        [0.0, 0.0, 0.0, 1.0],
        MovementSampleKind::Predicted,
    )?))?;

    assert!(presentation.frame(TickDelta::from_seconds(1.0 / 60.0)?)?);
    let frame = presentation
        .render_mailbox()
        .take_latest()?
        .ok_or_else(|| io::Error::other("render frame was not published"))?;
    assert_eq!(frame.id, RenderFrameId::new(1));
    assert_eq!(frame.views.len(), 1);
    assert_eq!(frame.instances.len(), 1);
    assert_eq!(frame.views[0].viewport, [0, 0, 1280, 720]);
    assert!(frame.views[0].view.into_iter().all(f32::is_finite));
    assert!(frame.views[0].projection.into_iter().all(f32::is_finite));
    assert_eq!(frame.instances[0].id, source.get());
    assert_eq!(frame.instances[0].resource, resource);
    assert_eq!(&frame.instances[0].transform[12..15], &[3.0, 4.0, 5.0]);
    Ok(())
}
