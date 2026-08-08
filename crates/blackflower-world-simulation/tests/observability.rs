#![cfg(all(feature = "metrics", feature = "tracing"))]

use std::fmt;
use std::sync::atomic::{AtomicU64, AtomicUsize, Ordering};
use std::sync::{Arc, Mutex};

use blackflower_world_simulation::SimulationWorld;
use metrics::{Counter, Gauge, Histogram, Key, KeyName, Metadata, Recorder, SharedString, Unit};
use tracing::field::{Field, Visit};
use tracing::span::{Attributes, Id, Record};
use tracing::{Event, Metadata as TracingMetadata, Subscriber};

type TestResult = Result<(), Box<dyn std::error::Error>>;

const EXPECTED_SYSTEM_ORDER: [&str; 62] = [
    "OpenTick",
    "ResetTickTransientStorage",
    "ActivateScheduledCommits",
    "CaptureCanonicalActorInputs",
    "CaptureEligibleDiscreteCommands",
    "DeriveLocomotionActions",
    "DeriveWeaponActions",
    "DeriveInteractionActions",
    "ResolveRewindRayCommands",
    "CatchUpLateBallistics",
    "CanonicalizeHistoricalCommandFacts",
    "ApplyCharacterControllerInputs",
    "ApplyQueuedPhenomenonEffects",
    "ApplyRigidBodyInputs",
    "AdvanceRigidBodyWorld",
    "RefreshCharacterGroundState",
    "CaptureRigidBodyState",
    "CaptureCharacterState",
    "CaptureContactFacts",
    "AdvanceBallistics",
    "ResolveExplosions",
    "ResolveMaterialResponses",
    "ResolveAssemblyDamage",
    "ResolveFractureAndBondFailures",
    "AdvanceAuthoritativeFireState",
    "AdvanceAuthoritativeSmokeField",
    "QueueRigidBodyEffects",
    "CapturePhenomenonFacts",
    "CaptureSoundEmissions",
    "ResolveAcousticPaths",
    "AdvanceAcousticPropagation",
    "BuildAcousticObservations",
    "CaptureAcousticFacts",
    "DeriveActorConditionTransitions",
    "DeriveWeaponStateTransitions",
    "DeriveInventoryStateTransitions",
    "DeriveWorldObjectStateTransitions",
    "DeriveDestructionTransitions",
    "DerivePhenomenonLifecycleTransitions",
    "CanonicalizeTransitionCandidates",
    "EvaluateTransitionPreconditions",
    "ResolveTransitionConflicts",
    "BuildTransitionCommit",
    "ValidateTransitionCommit",
    "CommitAcceptedTransitions",
    "CaptureCommittedTransitions",
    "DeriveSpatialStructureChanges",
    "UpdateCollisionStructure",
    "PublishNavigationChanges",
    "UpdateAcousticStructure",
    "UpdateAuthoritativeVisibilityStructure",
    "PublishSpatialStructureVersions",
    "CaptureSpatialStructureFacts",
    "ValidateAuthoritativeState",
    "CanonicalizeSimulationEvents",
    "ComputeAuthoritativeStateHash",
    "SealAuthoritativeState",
    "BuildTickOutputBatch",
    "BuildCommandDispositionOutput",
    "BuildDueReplicationView",
    "SealTickOutputBatch",
    "SubmitTickOutputBatch",
];

#[derive(Debug, Default)]
struct RecordingRecorder {
    system_executions: Arc<AtomicU64>,
}

impl Recorder for RecordingRecorder {
    fn describe_counter(&self, _key: KeyName, _unit: Option<Unit>, _description: SharedString) {}

    fn describe_gauge(&self, _key: KeyName, _unit: Option<Unit>, _description: SharedString) {}

    fn describe_histogram(&self, _key: KeyName, _unit: Option<Unit>, _description: SharedString) {}

    fn register_counter(&self, key: &Key, _metadata: &Metadata<'_>) -> Counter {
        if key.name() == "blackflower_world_simulation_system_executions_total" {
            Counter::from_arc(Arc::clone(&self.system_executions))
        } else {
            Counter::noop()
        }
    }

    fn register_gauge(&self, _key: &Key, _metadata: &Metadata<'_>) -> Gauge {
        Gauge::noop()
    }

    fn register_histogram(&self, _key: &Key, _metadata: &Metadata<'_>) -> Histogram {
        Histogram::noop()
    }
}

#[derive(Clone)]
struct CountingSubscriber {
    simulation_events: Arc<AtomicUsize>,
    simulation_systems: Arc<Mutex<Vec<String>>>,
    next_span_id: Arc<AtomicU64>,
}

impl Subscriber for CountingSubscriber {
    fn enabled(&self, _metadata: &TracingMetadata<'_>) -> bool {
        true
    }

    fn new_span(&self, _attributes: &Attributes<'_>) -> Id {
        let id = self.next_span_id.fetch_add(1, Ordering::Relaxed) + 1;
        Id::from_u64(id)
    }

    fn record(&self, _span: &Id, _values: &Record<'_>) {}

    fn record_follows_from(&self, _span: &Id, _follows: &Id) {}

    fn event(&self, event: &Event<'_>) {
        if event.metadata().target() == "blackflower_world_simulation" {
            self.simulation_events.fetch_add(1, Ordering::Relaxed);
            event.record(&mut SystemVisitor {
                systems: &self.simulation_systems,
            });
        }
    }

    fn enter(&self, _span: &Id) {}

    fn exit(&self, _span: &Id) {}
}

struct SystemVisitor<'a> {
    systems: &'a Mutex<Vec<String>>,
}

impl Visit for SystemVisitor<'_> {
    fn record_debug(&mut self, _field: &Field, _value: &dyn fmt::Debug) {}

    fn record_str(&mut self, field: &Field, value: &str) {
        if field.name() == "system"
            && let Ok(mut systems) = self.systems.lock()
        {
            systems.push(value.to_owned());
        }
    }
}

#[test]
fn registered_simulation_systems_emit_observability_signals() -> TestResult {
    let recorder = RecordingRecorder::default();
    let system_executions = Arc::clone(&recorder.system_executions);
    let simulation_events = Arc::new(AtomicUsize::new(0));
    let simulation_systems = Arc::new(Mutex::new(Vec::new()));
    let subscriber = CountingSubscriber {
        simulation_events: Arc::clone(&simulation_events),
        simulation_systems: Arc::clone(&simulation_systems),
        next_span_id: Arc::new(AtomicU64::new(0)),
    };

    tracing::subscriber::with_default(subscriber, || {
        metrics::with_local_recorder(&recorder, || -> TestResult {
            let mut simulation = SimulationWorld::new()?;
            assert!(simulation.tick()?);
            Ok(())
        })
    })?;

    let expected_count = u64::try_from(EXPECTED_SYSTEM_ORDER.len())?;
    assert_eq!(system_executions.load(Ordering::Relaxed), expected_count);
    assert!(simulation_events.load(Ordering::Relaxed) >= EXPECTED_SYSTEM_ORDER.len());
    assert_eq!(
        *simulation_systems
            .lock()
            .map_err(|_error| std::io::Error::other("system order lock poisoned"))?,
        EXPECTED_SYSTEM_ORDER,
    );
    Ok(())
}
