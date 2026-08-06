use std::collections::BTreeSet;
use std::io;
use std::num::{NonZeroU32, NonZeroUsize};
use std::sync::Mutex;
use std::time::Duration;

use blackflower_networking::{SessionState, SimulationTick};
use metrics::{Counter, Gauge, Histogram, Key, KeyName, Metadata, Recorder, SharedString, Unit};

use super::*;

#[derive(Debug, Default)]
struct RecordingRecorder {
    names: Mutex<BTreeSet<String>>,
}

impl RecordingRecorder {
    fn record(&self, key: &Key) {
        if let Ok(mut names) = self.names.lock() {
            names.insert(key.name().to_owned());
        }
    }

    fn names(&self) -> Result<BTreeSet<String>, io::Error> {
        self.names
            .lock()
            .map(|names| names.clone())
            .map_err(|_error| io::Error::other("metrics recorder lock poisoned"))
    }
}

impl Recorder for RecordingRecorder {
    fn describe_counter(&self, _key: KeyName, _unit: Option<Unit>, _description: SharedString) {}

    fn describe_gauge(&self, _key: KeyName, _unit: Option<Unit>, _description: SharedString) {}

    fn describe_histogram(&self, _key: KeyName, _unit: Option<Unit>, _description: SharedString) {}

    fn register_counter(&self, key: &Key, _metadata: &Metadata<'_>) -> Counter {
        self.record(key);
        Counter::noop()
    }

    fn register_gauge(&self, key: &Key, _metadata: &Metadata<'_>) -> Gauge {
        self.record(key);
        Gauge::noop()
    }

    fn register_histogram(&self, key: &Key, _metadata: &Metadata<'_>) -> Histogram {
        self.record(key);
        Histogram::noop()
    }
}

#[test]
fn agent_runtime_emits_every_owned_metric_family() -> Result<(), Box<dyn std::error::Error>> {
    let recorder = RecordingRecorder::default();
    metrics::with_local_recorder(&recorder, || -> Result<(), AgentDiagnosticError> {
        initialize_agent_metrics();
        let descriptor = descriptor()?;
        let mut diagnostics = AgentDiagnostics::connected(
            Some(AgentDiagnosticConfig::headless(descriptor)),
            SessionState::Active,
        );
        assert!(!diagnostics.records_enabled());
        diagnostics
            .record_sensorium_metrics(2, &[(MemoryKind::Spatial, MemoryStatus::Observed, 1)]);
        diagnostics.record_decision_metrics(
            PolicySource::Fallback,
            DecisionOutcome::Fallback,
            Duration::from_micros(150),
            Some(Duration::from_micros(120)),
            true,
            Some(FallbackReason::Budget),
        )?;
        diagnostics
            .record_navigation_query(NavigationQueryResult::Complete, Duration::from_micros(100));
        diagnostics.record_memory_eviction(MemoryEvictionReason::Expired);
        Ok(())
    })?;

    let names = recorder.names()?;
    for expected in [
        "blackflower_agent_active_agents",
        "blackflower_agent_agents",
        "blackflower_agent_decisions_total",
        "blackflower_agent_decision_duration_seconds",
        "blackflower_agent_inference_duration_seconds",
        "blackflower_agent_perceived_entities",
        "blackflower_agent_navigation_query_duration_seconds",
        "blackflower_agent_fallbacks_total",
        "blackflower_agent_decision_budget_exhaustions_total",
        "blackflower_agent_memory_items",
        "blackflower_agent_memory_evictions_total",
        "blackflower_agent_diagnostic_records_dropped_total",
    ] {
        assert!(names.contains(expected), "missing metric {expected}");
    }
    Ok(())
}

#[test]
fn bounded_stream_carries_exact_runtime_records() -> Result<(), Box<dyn std::error::Error>> {
    let capacity = NonZeroUsize::new(4).ok_or_else(|| std::io::Error::other("zero capacity"))?;
    let (sender, receiver) = agent_diagnostic_channel(capacity);
    let descriptor = descriptor()?;
    let agent_id = descriptor.id();
    let mut diagnostics = AgentDiagnostics::connected(
        Some(AgentDiagnosticConfig::new(descriptor, sender)),
        SessionState::Negotiating,
    );

    assert!(matches!(
        receiver.try_recv()?,
        AgentDiagnosticRecord::Status(_)
    ));
    diagnostics.record_sensorium(sensorium(agent_id)?)?;
    assert!(matches!(
        receiver.try_recv()?,
        AgentDiagnosticRecord::Sensorium(_)
    ));
    diagnostics.record_decision(decision(agent_id)?)?;
    assert!(matches!(
        receiver.try_recv()?,
        AgentDiagnosticRecord::Decision(_)
    ));
    Ok(())
}

#[test]
fn full_diagnostic_queue_never_blocks_the_runtime() -> Result<(), Box<dyn std::error::Error>> {
    let (sender, receiver) = agent_diagnostic_channel(NonZeroUsize::MIN);
    let descriptor = descriptor()?;
    let agent_id = descriptor.id();
    let mut diagnostics = AgentDiagnostics::connected(
        Some(AgentDiagnosticConfig::new(descriptor, sender)),
        SessionState::Negotiating,
    );

    diagnostics.record_sensorium(sensorium(agent_id)?)?;
    assert!(matches!(
        receiver.try_recv()?,
        AgentDiagnosticRecord::Status(_)
    ));
    assert!(matches!(
        receiver.try_recv(),
        Err(std::sync::mpsc::TryRecvError::Empty)
    ));
    Ok(())
}

#[test]
fn snapshots_reject_duplicate_channels_and_unclassified_fallbacks()
-> Result<(), Box<dyn std::error::Error>> {
    assert!(matches!(
        DiagnosticText::new("line\nbreak"),
        Err(AgentDiagnosticError::InvalidTextCharacter)
    ));
    let agent_id = agent_id();
    let channel = SensoriumChannelSnapshot::new(
        SensoriumChannelKind::Vision,
        SensoriumAvailability::Admitted,
        text("two visible silhouettes")?,
        true,
    );
    let duplicate = SensoriumSnapshot::new(
        agent_id,
        1,
        SimulationTick::new(4),
        1,
        text("classical-v1")?,
        2,
        vec![channel.clone(), channel],
        Vec::new(),
    );
    assert!(matches!(
        duplicate,
        Err(AgentDiagnosticError::DuplicateSensoriumChannel)
    ));

    let fallback = DecisionRecord::new(
        agent_id,
        1,
        1,
        SimulationTick::new(4),
        text("hold cover")?,
        PolicySource::Fallback,
        DecisionOutcome::Fallback,
        text("neutral input")?,
        text("accepted")?,
        Vec::new(),
        Vec::new(),
        Duration::from_micros(50),
        None,
        false,
        None,
    );
    assert!(matches!(
        fallback,
        Err(AgentDiagnosticError::MissingFallbackReason)
    ));
    Ok(())
}

fn descriptor() -> Result<AgentDescriptor, AgentDiagnosticError> {
    Ok(AgentDescriptor::new(
        agent_id(),
        text("standard")?,
        text("classical-v1")?,
    ))
}

fn sensorium(agent_id: AgentId) -> Result<SensoriumSnapshot, AgentDiagnosticError> {
    let channels = vec![SensoriumChannelSnapshot::new(
        SensoriumChannelKind::Vision,
        SensoriumAvailability::Admitted,
        text("two visible silhouettes")?,
        true,
    )];
    let memory = vec![MemoryItemSnapshot::new(
        1,
        MemoryKind::Spatial,
        MemoryStatus::Observed,
        text("cover edge ahead")?,
        0.8,
        0.2,
        Duration::from_millis(40),
        true,
    )?];
    SensoriumSnapshot::new(
        agent_id,
        1,
        SimulationTick::new(4),
        1,
        text("classical-v1")?,
        2,
        channels,
        memory,
    )
}

fn decision(agent_id: AgentId) -> Result<DecisionRecord, AgentDiagnosticError> {
    DecisionRecord::new(
        agent_id,
        1,
        1,
        SimulationTick::new(4),
        text("take cover")?,
        PolicySource::Classical,
        DecisionOutcome::Completed,
        text("move left")?,
        text("input 8 accepted")?,
        vec![DecisionCandidate::new(
            text("move left")?,
            0.75,
            text("selected")?,
        )?],
        vec![DecisionConstraint::new(
            text("reaction gate")?,
            text("unchanged")?,
        )],
        Duration::from_micros(80),
        None,
        false,
        None,
    )
}

fn agent_id() -> AgentId {
    AgentId::new(NonZeroU32::MIN)
}

fn text(value: &str) -> Result<DiagnosticText, AgentDiagnosticError> {
    DiagnosticText::new(value)
}
