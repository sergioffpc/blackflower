use std::collections::BTreeSet;
use std::fmt;
use std::num::{NonZeroU32, NonZeroUsize};
use std::sync::mpsc::{self, Receiver, SyncSender, TryRecvError, TrySendError};
use std::time::{Duration, Instant};

use blackflower_networking::{SessionState, SimulationTick};
use metrics::Unit;

const MAX_DIAGNOSTIC_TEXT_BYTES: usize = 96;
const MAX_SENSORIUM_CHANNELS: usize = 8;
const MAX_MEMORY_ITEMS: usize = 32;
const MAX_DECISION_CANDIDATES: usize = 8;
const MAX_DECISION_CONSTRAINTS: usize = 12;
const MEMORY_KIND_COUNT: usize = 4;
const MEMORY_STATUS_COUNT: usize = 4;
const MEMORY_METRIC_SERIES: usize = MEMORY_KIND_COUNT * MEMORY_STATUS_COUNT;

/// Process-local pseudonymous identity used only by agent diagnostics.
#[derive(Debug, Clone, Copy, PartialEq, Eq, PartialOrd, Ord, Hash)]
pub struct AgentId(NonZeroU32);

impl AgentId {
    /// Construct a non-zero process-local agent identity.
    #[must_use]
    pub const fn new(value: NonZeroU32) -> Self {
        Self(value)
    }

    /// Return the numeric process-local identity.
    #[must_use]
    pub const fn get(self) -> u32 {
        self.0.get()
    }
}

impl fmt::Display for AgentId {
    fn fmt(&self, formatter: &mut fmt::Formatter<'_>) -> fmt::Result {
        write!(formatter, "{:02}", self.get())
    }
}

/// Bounded operator-facing text carried by the process-local diagnostic stream.
#[derive(Debug, Clone, PartialEq, Eq, PartialOrd, Ord)]
pub struct DiagnosticText(String);

impl DiagnosticText {
    /// Validate one non-empty bounded diagnostic value.
    pub fn new(value: impl Into<String>) -> Result<Self, AgentDiagnosticError> {
        let value = value.into();
        if value.is_empty() || value.len() > MAX_DIAGNOSTIC_TEXT_BYTES {
            return Err(AgentDiagnosticError::InvalidTextLength {
                actual: value.len(),
                maximum: MAX_DIAGNOSTIC_TEXT_BYTES,
            });
        }
        if value.chars().any(char::is_control) {
            return Err(AgentDiagnosticError::InvalidTextCharacter);
        }
        Ok(Self(value))
    }

    /// Borrow the validated text.
    #[must_use]
    pub fn as_str(&self) -> &str {
        &self.0
    }
}

impl fmt::Display for DiagnosticText {
    fn fmt(&self, formatter: &mut fmt::Formatter<'_>) -> fmt::Result {
        self.0.fmt(formatter)
    }
}

/// Static operator identity supplied by the real agent controller.
#[derive(Debug, Clone, PartialEq, Eq)]
pub struct AgentDescriptor {
    id: AgentId,
    difficulty: DiagnosticText,
    policy_version: DiagnosticText,
}

impl AgentDescriptor {
    /// Bind a process-local identity to configured difficulty and policy versions.
    #[must_use]
    pub const fn new(
        id: AgentId,
        difficulty: DiagnosticText,
        policy_version: DiagnosticText,
    ) -> Self {
        Self {
            id,
            difficulty,
            policy_version,
        }
    }

    /// Return the process-local identity.
    #[must_use]
    pub const fn id(&self) -> AgentId {
        self.id
    }

    /// Return the configured difficulty label.
    #[must_use]
    pub const fn difficulty(&self) -> &DiagnosticText {
        &self.difficulty
    }

    /// Return the active policy or model version.
    #[must_use]
    pub const fn policy_version(&self) -> &DiagnosticText {
        &self.policy_version
    }
}

/// Bounded health classification used for aggregate metrics and live status.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
#[non_exhaustive]
pub enum AgentHealth {
    /// Runtime is establishing or synchronizing the ordinary-client session.
    Starting,
    /// Runtime and controller are making expected progress.
    Healthy,
    /// Progress has exceeded the controller's bounded freshness threshold.
    Stalled,
    /// A bounded fallback is currently protecting the input boundary.
    Fallback,
}

impl AgentHealth {
    /// Stable low-cardinality metric value.
    #[must_use]
    pub const fn as_str(self) -> &'static str {
        match self {
            Self::Starting => "starting",
            Self::Healthy => "healthy",
            Self::Stalled => "stalled",
            Self::Fallback => "fallback",
        }
    }
}

/// One immutable lifecycle view emitted by an established agent runtime.
#[derive(Debug, Clone)]
pub struct AgentStatusSnapshot {
    descriptor: AgentDescriptor,
    session_state: SessionState,
    health: AgentHealth,
    recorded_at: Instant,
}

impl AgentStatusSnapshot {
    /// Return the static agent descriptor.
    #[must_use]
    pub const fn descriptor(&self) -> &AgentDescriptor {
        &self.descriptor
    }

    /// Return the ordinary-client session lifecycle state.
    #[must_use]
    pub const fn session_state(&self) -> SessionState {
        self.session_state
    }

    /// Return the controller health classification.
    #[must_use]
    pub const fn health(&self) -> AgentHealth {
        self.health
    }

    /// Return the process-monotonic capture time.
    #[must_use]
    pub const fn recorded_at(&self) -> Instant {
        self.recorded_at
    }
}

/// Stable semantic sensorium group rendered by the foreground UI.
#[derive(Debug, Clone, Copy, PartialEq, Eq, PartialOrd, Ord)]
#[non_exhaustive]
pub enum SensoriumChannelKind {
    /// Gameplay-admitted semantic visual evidence.
    Vision,
    /// Privacy-preserving acoustic observations.
    Hearing,
    /// Proprioception, impacts, condition, and thermal state.
    Body,
    /// Equipment and currently available action capacity.
    Capacity,
    /// Semantic memory and belief state used by policy.
    Memory,
    /// Gameplay-supplied effective performance derivation.
    Performance,
}

impl SensoriumChannelKind {
    /// Stable operator label.
    #[must_use]
    pub const fn as_str(self) -> &'static str {
        match self {
            Self::Vision => "vision",
            Self::Hearing => "hearing",
            Self::Body => "body",
            Self::Capacity => "capacity",
            Self::Memory => "memory",
            Self::Performance => "performance",
        }
    }
}

/// Capability state for one gameplay-owned sensorium channel.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
#[non_exhaustive]
pub enum SensoriumAvailability {
    /// Gameplay does not expose this channel to a human or agent yet.
    Unavailable,
    /// A value exists but is older than the gameplay-owned freshness contract.
    Stale,
    /// A newer value exists but has not passed the modeled reaction gate.
    ReactionGated,
    /// The exact value shown was admitted to the current policy decision.
    Admitted,
}

impl SensoriumAvailability {
    /// Stable operator label.
    #[must_use]
    pub const fn as_str(self) -> &'static str {
        match self {
            Self::Unavailable => "unavailable",
            Self::Stale => "stale",
            Self::ReactionGated => "reaction gated",
            Self::Admitted => "admitted",
        }
    }
}

/// Exact bounded summary of one sensorium channel produced by gameplay.
#[derive(Debug, Clone, PartialEq, Eq)]
pub struct SensoriumChannelSnapshot {
    kind: SensoriumChannelKind,
    availability: SensoriumAvailability,
    summary: DiagnosticText,
    affected_decision: bool,
}

impl SensoriumChannelSnapshot {
    /// Construct a channel projection without recomputing gameplay semantics.
    #[must_use]
    pub const fn new(
        kind: SensoriumChannelKind,
        availability: SensoriumAvailability,
        summary: DiagnosticText,
        affected_decision: bool,
    ) -> Self {
        Self {
            kind,
            availability,
            summary,
            affected_decision,
        }
    }

    /// Return the semantic channel.
    #[must_use]
    pub const fn kind(&self) -> SensoriumChannelKind {
        self.kind
    }

    /// Return the capability/freshness state.
    #[must_use]
    pub const fn availability(&self) -> SensoriumAvailability {
        self.availability
    }

    /// Return the gameplay-produced bounded summary.
    #[must_use]
    pub const fn summary(&self) -> &DiagnosticText {
        &self.summary
    }

    /// Return whether a deterministic constraint applied this channel to the decision.
    #[must_use]
    pub const fn affected_decision(&self) -> bool {
        self.affected_decision
    }
}

/// Stable semantic-memory class used by metrics and live records.
#[derive(Debug, Clone, Copy, PartialEq, Eq, PartialOrd, Ord)]
#[non_exhaustive]
pub enum MemoryKind {
    /// Short-lived legal sensory evidence.
    Sensory,
    /// Last-known or inferred relative spatial belief.
    Spatial,
    /// Bounded recent event or outcome.
    Episodic,
    /// Current goal, plan, target token, or unresolved task.
    Working,
}

impl MemoryKind {
    /// Stable low-cardinality metric value.
    #[must_use]
    pub const fn as_str(self) -> &'static str {
        match self {
            Self::Sensory => "sensory",
            Self::Spatial => "spatial",
            Self::Episodic => "episodic",
            Self::Working => "working",
        }
    }

    const fn index(self) -> usize {
        match self {
            Self::Sensory => 0,
            Self::Spatial => 1,
            Self::Episodic => 2,
            Self::Working => 3,
        }
    }
}

/// Stable memory-evidence state.
#[derive(Debug, Clone, Copy, PartialEq, Eq, PartialOrd, Ord)]
#[non_exhaustive]
pub enum MemoryStatus {
    /// Evidence is directly observed and legal now.
    Observed,
    /// Evidence is retained from an earlier legal observation.
    Remembered,
    /// Evidence is explicitly inferred with bounded uncertainty.
    Inferred,
    /// Evidence is expired but retained in the bounded diagnostic timeline.
    Expired,
}

impl MemoryStatus {
    /// Stable low-cardinality metric value.
    #[must_use]
    pub const fn as_str(self) -> &'static str {
        match self {
            Self::Observed => "observed",
            Self::Remembered => "remembered",
            Self::Inferred => "inferred",
            Self::Expired => "expired",
        }
    }

    const fn index(self) -> usize {
        match self {
            Self::Observed => 0,
            Self::Remembered => 1,
            Self::Inferred => 2,
            Self::Expired => 3,
        }
    }
}

const MEMORY_KINDS: [MemoryKind; MEMORY_KIND_COUNT] = [
    MemoryKind::Sensory,
    MemoryKind::Spatial,
    MemoryKind::Episodic,
    MemoryKind::Working,
];
const MEMORY_STATUSES: [MemoryStatus; MEMORY_STATUS_COUNT] = [
    MemoryStatus::Observed,
    MemoryStatus::Remembered,
    MemoryStatus::Inferred,
    MemoryStatus::Expired,
];

/// Bounded, redacted semantic-memory item used by the selected agent view.
#[derive(Debug, Clone)]
pub struct MemoryItemSnapshot {
    token: u64,
    kind: MemoryKind,
    status: MemoryStatus,
    summary: DiagnosticText,
    confidence: f32,
    uncertainty: f32,
    age: Duration,
    consumed_by_decision: bool,
}

impl MemoryItemSnapshot {
    /// Construct one finite, normalized, process-local memory projection.
    #[allow(
        clippy::too_many_arguments,
        reason = "the constructor freezes one explicit semantic-memory diagnostic item"
    )]
    pub fn new(
        token: u64,
        kind: MemoryKind,
        status: MemoryStatus,
        summary: DiagnosticText,
        confidence: f32,
        uncertainty: f32,
        age: Duration,
        consumed_by_decision: bool,
    ) -> Result<Self, AgentDiagnosticError> {
        if token == 0 {
            return Err(AgentDiagnosticError::ZeroMemoryToken);
        }
        if !is_normalized(confidence) || !is_normalized(uncertainty) {
            return Err(AgentDiagnosticError::InvalidNormalizedValue);
        }
        Ok(Self {
            token,
            kind,
            status,
            summary,
            confidence,
            uncertainty,
            age,
            consumed_by_decision,
        })
    }

    /// Return the process-local non-authoritative token.
    #[must_use]
    pub const fn token(&self) -> u64 {
        self.token
    }

    /// Return the semantic memory class.
    #[must_use]
    pub const fn kind(&self) -> MemoryKind {
        self.kind
    }

    /// Return the current evidence state.
    #[must_use]
    pub const fn status(&self) -> MemoryStatus {
        self.status
    }

    /// Return the bounded legal-evidence summary.
    #[must_use]
    pub const fn summary(&self) -> &DiagnosticText {
        &self.summary
    }

    /// Return the normalized memory confidence.
    #[must_use]
    pub const fn confidence(&self) -> f32 {
        self.confidence
    }

    /// Return the normalized memory uncertainty.
    #[must_use]
    pub const fn uncertainty(&self) -> f32 {
        self.uncertainty
    }

    /// Return age relative to the captured observation.
    #[must_use]
    pub const fn age(&self) -> Duration {
        self.age
    }

    /// Return whether this exact item was included in the displayed decision.
    #[must_use]
    pub const fn consumed_by_decision(&self) -> bool {
        self.consumed_by_decision
    }
}

/// Immutable gameplay-owned sensorium projection consumed by foreground diagnostics.
#[derive(Debug, Clone)]
pub struct SensoriumSnapshot {
    agent_id: AgentId,
    observation_sequence: u64,
    observation_tick: SimulationTick,
    schema_version: u16,
    policy_version: DiagnosticText,
    perceived_entities: u16,
    channels: Vec<SensoriumChannelSnapshot>,
    memory: Vec<MemoryItemSnapshot>,
    recorded_at: Instant,
}

impl SensoriumSnapshot {
    /// Validate and freeze one already-computed sensorium diagnostic projection.
    #[allow(
        clippy::too_many_arguments,
        reason = "the constructor captures one explicit immutable diagnostic boundary"
    )]
    pub fn new(
        agent_id: AgentId,
        observation_sequence: u64,
        observation_tick: SimulationTick,
        schema_version: u16,
        policy_version: DiagnosticText,
        perceived_entities: u16,
        channels: Vec<SensoriumChannelSnapshot>,
        memory: Vec<MemoryItemSnapshot>,
    ) -> Result<Self, AgentDiagnosticError> {
        if observation_sequence == 0 {
            return Err(AgentDiagnosticError::ZeroObservationSequence);
        }
        if schema_version == 0 {
            return Err(AgentDiagnosticError::ZeroSchemaVersion);
        }
        if channels.len() > MAX_SENSORIUM_CHANNELS {
            return Err(AgentDiagnosticError::CollectionTooLarge {
                collection: "sensorium channels",
                actual: channels.len(),
                maximum: MAX_SENSORIUM_CHANNELS,
            });
        }
        if memory.len() > MAX_MEMORY_ITEMS {
            return Err(AgentDiagnosticError::CollectionTooLarge {
                collection: "memory items",
                actual: memory.len(),
                maximum: MAX_MEMORY_ITEMS,
            });
        }
        let mut channel_kinds = BTreeSet::new();
        if channels
            .iter()
            .any(|channel| !channel_kinds.insert(channel.kind()))
        {
            return Err(AgentDiagnosticError::DuplicateSensoriumChannel);
        }
        let mut memory_tokens = BTreeSet::new();
        if memory
            .iter()
            .any(|item| !memory_tokens.insert(item.token()))
        {
            return Err(AgentDiagnosticError::DuplicateMemoryToken);
        }
        Ok(Self {
            agent_id,
            observation_sequence,
            observation_tick,
            schema_version,
            policy_version,
            perceived_entities,
            channels,
            memory,
            recorded_at: Instant::now(),
        })
    }

    /// Return the process-local agent identity.
    #[must_use]
    pub const fn agent_id(&self) -> AgentId {
        self.agent_id
    }

    /// Return the monotonically increasing local observation sequence.
    #[must_use]
    pub const fn observation_sequence(&self) -> u64 {
        self.observation_sequence
    }

    /// Return the source simulation tick.
    #[must_use]
    pub const fn observation_tick(&self) -> SimulationTick {
        self.observation_tick
    }

    /// Return the gameplay-owned sensorium schema version.
    #[must_use]
    pub const fn schema_version(&self) -> u16 {
        self.schema_version
    }

    /// Return the policy version that admitted this observation.
    #[must_use]
    pub const fn policy_version(&self) -> &DiagnosticText {
        &self.policy_version
    }

    /// Return the number of entities admitted by the real sensory filter.
    #[must_use]
    pub const fn perceived_entities(&self) -> u16 {
        self.perceived_entities
    }

    /// Return the bounded capability-driven channel projections.
    #[must_use]
    pub fn channels(&self) -> &[SensoriumChannelSnapshot] {
        &self.channels
    }

    /// Return the exact bounded semantic memory used by the controller.
    #[must_use]
    pub fn memory(&self) -> &[MemoryItemSnapshot] {
        &self.memory
    }

    /// Return the process-monotonic capture time.
    #[must_use]
    pub const fn recorded_at(&self) -> Instant {
        self.recorded_at
    }
}

/// Bounded source of the selected semantic decision.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
#[non_exhaustive]
pub enum PolicySource {
    /// Classical behavior policy selected a new action.
    Classical,
    /// A local neural-network policy selected a new action.
    NeuralNetwork,
    /// The controller continued a previously selected bounded plan.
    Held,
    /// A bounded safety fallback replaced normal policy output.
    Fallback,
}

impl PolicySource {
    /// Stable low-cardinality metric value.
    #[must_use]
    pub const fn as_str(self) -> &'static str {
        match self {
            Self::Classical => "classical",
            Self::NeuralNetwork => "nn",
            Self::Held => "held",
            Self::Fallback => "fallback",
        }
    }
}

/// Bounded outcome of one scheduled decision.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
#[non_exhaustive]
pub enum DecisionOutcome {
    /// A new semantic action was emitted successfully.
    Completed,
    /// A prior bounded plan remained active.
    Held,
    /// The harness or action validator rejected the result.
    Rejected,
    /// A neutral or classical fallback was emitted.
    Fallback,
}

impl DecisionOutcome {
    /// Stable low-cardinality metric value.
    #[must_use]
    pub const fn as_str(self) -> &'static str {
        match self {
            Self::Completed => "completed",
            Self::Held => "held",
            Self::Rejected => "rejected",
            Self::Fallback => "fallback",
        }
    }
}

/// Stable bounded fallback reason.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
#[non_exhaustive]
pub enum FallbackReason {
    /// No legal policy result was available before the decision deadline.
    Budget,
    /// Policy or inference produced an unusable result.
    Policy,
    /// Navigation could not produce a legal route or steering result.
    Navigation,
    /// Gameplay action validation rejected the selected action.
    InvalidAction,
    /// The ordinary-client session was not ready for input.
    Session,
}

impl FallbackReason {
    /// Stable low-cardinality metric value.
    #[must_use]
    pub const fn as_str(self) -> &'static str {
        match self {
            Self::Budget => "budget",
            Self::Policy => "policy",
            Self::Navigation => "navigation",
            Self::InvalidAction => "invalid_action",
            Self::Session => "session",
        }
    }
}

/// One bounded candidate copied from the exact policy evaluation.
#[derive(Debug, Clone, PartialEq)]
pub struct DecisionCandidate {
    action: DiagnosticText,
    score: f32,
    disposition: DiagnosticText,
}

impl DecisionCandidate {
    /// Construct a finite candidate projection.
    pub fn new(
        action: DiagnosticText,
        score: f32,
        disposition: DiagnosticText,
    ) -> Result<Self, AgentDiagnosticError> {
        if !score.is_finite() {
            return Err(AgentDiagnosticError::NonFiniteScore);
        }
        Ok(Self {
            action,
            score,
            disposition,
        })
    }

    /// Return the semantic action label owned by gameplay.
    #[must_use]
    pub const fn action(&self) -> &DiagnosticText {
        &self.action
    }

    /// Return the policy score without claiming calibrated confidence.
    #[must_use]
    pub const fn score(&self) -> f32 {
        self.score
    }

    /// Return the stable bounded selection or rejection disposition.
    #[must_use]
    pub const fn disposition(&self) -> &DiagnosticText {
        &self.disposition
    }
}

/// One deterministic downstream change to a selected semantic action.
#[derive(Debug, Clone, PartialEq, Eq)]
pub struct DecisionConstraint {
    stage: DiagnosticText,
    effect: DiagnosticText,
}

impl DecisionConstraint {
    /// Construct an ordered constraint projection.
    #[must_use]
    pub const fn new(stage: DiagnosticText, effect: DiagnosticText) -> Self {
        Self { stage, effect }
    }

    /// Return the stable constraint stage.
    #[must_use]
    pub const fn stage(&self) -> &DiagnosticText {
        &self.stage
    }

    /// Return the exact bounded effect applied at that stage.
    #[must_use]
    pub const fn effect(&self) -> &DiagnosticText {
        &self.effect
    }
}

/// Immutable causal record of one already-completed real controller decision.
#[derive(Debug, Clone)]
pub struct DecisionRecord {
    agent_id: AgentId,
    decision_sequence: u64,
    observation_sequence: u64,
    observation_tick: SimulationTick,
    current_intent: DiagnosticText,
    source: PolicySource,
    outcome: DecisionOutcome,
    selected_action: DiagnosticText,
    emission: DiagnosticText,
    candidates: Vec<DecisionCandidate>,
    constraints: Vec<DecisionConstraint>,
    decision_duration: Duration,
    inference_duration: Option<Duration>,
    budget_exhausted: bool,
    fallback_reason: Option<FallbackReason>,
    recorded_at: Instant,
}

impl DecisionRecord {
    /// Validate and freeze one exact decision chain after input submission.
    #[allow(
        clippy::too_many_arguments,
        reason = "the constructor captures the complete bounded decision correlation and result"
    )]
    pub fn new(
        agent_id: AgentId,
        decision_sequence: u64,
        observation_sequence: u64,
        observation_tick: SimulationTick,
        current_intent: DiagnosticText,
        source: PolicySource,
        outcome: DecisionOutcome,
        selected_action: DiagnosticText,
        emission: DiagnosticText,
        candidates: Vec<DecisionCandidate>,
        constraints: Vec<DecisionConstraint>,
        decision_duration: Duration,
        inference_duration: Option<Duration>,
        budget_exhausted: bool,
        fallback_reason: Option<FallbackReason>,
    ) -> Result<Self, AgentDiagnosticError> {
        if decision_sequence == 0 {
            return Err(AgentDiagnosticError::ZeroDecisionSequence);
        }
        if observation_sequence == 0 {
            return Err(AgentDiagnosticError::ZeroObservationSequence);
        }
        if candidates.len() > MAX_DECISION_CANDIDATES {
            return Err(AgentDiagnosticError::CollectionTooLarge {
                collection: "decision candidates",
                actual: candidates.len(),
                maximum: MAX_DECISION_CANDIDATES,
            });
        }
        if constraints.len() > MAX_DECISION_CONSTRAINTS {
            return Err(AgentDiagnosticError::CollectionTooLarge {
                collection: "decision constraints",
                actual: constraints.len(),
                maximum: MAX_DECISION_CONSTRAINTS,
            });
        }
        if outcome == DecisionOutcome::Fallback && fallback_reason.is_none() {
            return Err(AgentDiagnosticError::MissingFallbackReason);
        }
        Ok(Self {
            agent_id,
            decision_sequence,
            observation_sequence,
            observation_tick,
            current_intent,
            source,
            outcome,
            selected_action,
            emission,
            candidates,
            constraints,
            decision_duration,
            inference_duration,
            budget_exhausted,
            fallback_reason,
            recorded_at: Instant::now(),
        })
    }

    /// Return the process-local agent identity.
    #[must_use]
    pub const fn agent_id(&self) -> AgentId {
        self.agent_id
    }

    /// Return the monotonically increasing local decision sequence.
    #[must_use]
    pub const fn decision_sequence(&self) -> u64 {
        self.decision_sequence
    }

    /// Return the exact observation sequence admitted to policy.
    #[must_use]
    pub const fn observation_sequence(&self) -> u64 {
        self.observation_sequence
    }

    /// Return the source observation tick.
    #[must_use]
    pub const fn observation_tick(&self) -> SimulationTick {
        self.observation_tick
    }

    /// Return the current semantic intent.
    #[must_use]
    pub const fn current_intent(&self) -> &DiagnosticText {
        &self.current_intent
    }

    /// Return the policy source.
    #[must_use]
    pub const fn source(&self) -> PolicySource {
        self.source
    }

    /// Return the terminal decision outcome.
    #[must_use]
    pub const fn outcome(&self) -> DecisionOutcome {
        self.outcome
    }

    /// Return the semantic action selected before downstream emission.
    #[must_use]
    pub const fn selected_action(&self) -> &DiagnosticText {
        &self.selected_action
    }

    /// Return the bounded accepted/rejected control emission summary.
    #[must_use]
    pub const fn emission(&self) -> &DiagnosticText {
        &self.emission
    }

    /// Return bounded policy candidates in evaluation order.
    #[must_use]
    pub fn candidates(&self) -> &[DecisionCandidate] {
        &self.candidates
    }

    /// Return deterministic downstream constraints in application order.
    #[must_use]
    pub fn constraints(&self) -> &[DecisionConstraint] {
        &self.constraints
    }

    /// Return end-to-end controller decision duration.
    #[must_use]
    pub const fn decision_duration(&self) -> Duration {
        self.decision_duration
    }

    /// Return local NN inference duration when an NN was actually invoked.
    #[must_use]
    pub const fn inference_duration(&self) -> Option<Duration> {
        self.inference_duration
    }

    /// Return whether the shared scheduler budget was exhausted.
    #[must_use]
    pub const fn budget_exhausted(&self) -> bool {
        self.budget_exhausted
    }

    /// Return the bounded fallback reason, if any.
    #[must_use]
    pub const fn fallback_reason(&self) -> Option<FallbackReason> {
        self.fallback_reason
    }

    /// Return the process-monotonic capture time.
    #[must_use]
    pub const fn recorded_at(&self) -> Instant {
        self.recorded_at
    }
}

/// One process-local immutable foreground diagnostic record.
#[derive(Debug, Clone)]
pub enum AgentDiagnosticRecord {
    /// Runtime lifecycle or health changed.
    Status(AgentStatusSnapshot),
    /// A gameplay-owned sensorium snapshot was admitted to the controller.
    Sensorium(SensoriumSnapshot),
    /// One completed controller decision and its downstream result.
    Decision(DecisionRecord),
}

impl AgentDiagnosticRecord {
    /// Return the bounded record kind used by drop metrics.
    #[must_use]
    pub const fn kind(&self) -> DiagnosticRecordKind {
        match self {
            Self::Status(_) => DiagnosticRecordKind::Status,
            Self::Sensorium(_) => DiagnosticRecordKind::Sensorium,
            Self::Decision(_) => DiagnosticRecordKind::Decision,
        }
    }
}

/// Stable bounded diagnostic queue record kind.
#[derive(Debug, Clone, Copy, PartialEq, Eq, PartialOrd, Ord)]
pub enum DiagnosticRecordKind {
    /// Runtime lifecycle/health snapshot.
    Status,
    /// Sensorium and semantic-memory snapshot.
    Sensorium,
    /// Completed decision record.
    Decision,
}

impl DiagnosticRecordKind {
    /// Stable low-cardinality metric value.
    #[must_use]
    pub const fn as_str(self) -> &'static str {
        match self {
            Self::Status => "status",
            Self::Sensorium => "sensorium",
            Self::Decision => "decision",
        }
    }
}

/// Cloneable producer endpoint for one bounded foreground diagnostic channel.
#[derive(Debug, Clone)]
pub struct AgentDiagnosticSender(SyncSender<AgentDiagnosticRecord>);

/// Single-consumer endpoint owned by the foreground terminal UI.
#[derive(Debug)]
pub struct AgentDiagnosticReceiver(Receiver<AgentDiagnosticRecord>);

impl AgentDiagnosticReceiver {
    /// Receive one queued record without blocking the terminal loop.
    pub fn try_recv(&self) -> Result<AgentDiagnosticRecord, TryRecvError> {
        self.0.try_recv()
    }
}

/// Create a process-local bounded, lossy, non-blocking diagnostic channel.
#[must_use]
pub fn agent_diagnostic_channel(
    capacity: NonZeroUsize,
) -> (AgentDiagnosticSender, AgentDiagnosticReceiver) {
    let (sender, receiver) = mpsc::sync_channel(capacity.get());
    (
        AgentDiagnosticSender(sender),
        AgentDiagnosticReceiver(receiver),
    )
}

/// Optional real-runtime identity and stream installed by the hosting executable.
#[derive(Debug, Clone)]
pub struct AgentDiagnosticConfig {
    descriptor: AgentDescriptor,
    sender: Option<AgentDiagnosticSender>,
}

impl AgentDiagnosticConfig {
    /// Configure real-runtime identity and aggregate metrics without a detail observer.
    #[must_use]
    pub const fn headless(descriptor: AgentDescriptor) -> Self {
        Self {
            descriptor,
            sender: None,
        }
    }

    /// Bind real runtime records to a bounded foreground stream.
    #[must_use]
    pub const fn new(descriptor: AgentDescriptor, sender: AgentDiagnosticSender) -> Self {
        Self {
            descriptor,
            sender: Some(sender),
        }
    }
}

/// Per-runtime metrics and optional non-blocking foreground observer.
pub struct AgentDiagnostics {
    descriptor: Option<AgentDescriptor>,
    sender: Option<AgentDiagnosticSender>,
    session_state: SessionState,
    health: AgentHealth,
    memory_counts: [u32; MEMORY_METRIC_SERIES],
    saturated: BTreeSet<DiagnosticRecordKind>,
}

impl AgentDiagnostics {
    pub(crate) fn connected(
        config: Option<AgentDiagnosticConfig>,
        session_state: SessionState,
    ) -> Self {
        describe_agent_metrics();
        metrics::gauge!("blackflower_agent_active_agents").increment(1.0);
        metrics::gauge!(
            "blackflower_agent_agents",
            "health" => AgentHealth::Starting.as_str()
        )
        .increment(1.0);
        let (descriptor, sender) = config.map_or((None, None), |config| {
            (Some(config.descriptor), config.sender)
        });
        let mut diagnostics = Self {
            descriptor,
            sender,
            session_state,
            health: AgentHealth::Starting,
            memory_counts: [0; MEMORY_METRIC_SERIES],
            saturated: BTreeSet::new(),
        };
        diagnostics.publish_status();
        tracing::info!(
            target: "blackflower_agent",
            event_name = "agent_runtime_connected",
            "agent runtime connected",
        );
        diagnostics
    }

    /// Record an ordinary-client session transition when it changes.
    pub fn set_session_state(&mut self, session_state: SessionState) {
        if self.session_state == session_state {
            return;
        }
        self.session_state = session_state;
        if session_state == SessionState::Active && self.health == AgentHealth::Starting {
            self.replace_health(AgentHealth::Healthy);
        }
        self.publish_status();
    }

    /// Return whether a live observer justifies constructing detailed snapshots.
    ///
    /// Controllers use this before allocating or cloning diagnostic-only DTOs.
    /// Aggregate metric methods remain available independently.
    #[must_use]
    pub const fn records_enabled(&self) -> bool {
        self.sender.is_some()
    }

    /// Record a bounded controller health transition.
    pub fn set_health(&mut self, health: AgentHealth) {
        if self.health == health {
            return;
        }
        self.replace_health(health);
        self.publish_status();
        if matches!(health, AgentHealth::Stalled | AgentHealth::Fallback) {
            tracing::warn!(
                target: "blackflower_agent",
                event_name = "agent_health_degraded",
                health = health.as_str(),
                "agent health degraded",
            );
        } else {
            tracing::info!(
                target: "blackflower_agent",
                event_name = "agent_health_recovered",
                health = health.as_str(),
                "agent health recovered",
            );
        }
    }

    /// Emit one exact gameplay-produced sensorium and memory projection.
    pub fn record_sensorium(
        &mut self,
        snapshot: SensoriumSnapshot,
    ) -> Result<(), AgentDiagnosticError> {
        self.ensure_agent(snapshot.agent_id())?;
        metrics::histogram!("blackflower_agent_perceived_entities")
            .record(f64::from(snapshot.perceived_entities()));
        let mut next = [0_u32; MEMORY_METRIC_SERIES];
        for item in snapshot.memory() {
            let slot = memory_slot(item.kind(), item.status());
            next[slot] = next[slot].saturating_add(1);
        }
        self.replace_memory_counts(next);
        self.publish(AgentDiagnosticRecord::Sensorium(snapshot));
        Ok(())
    }

    /// Record controller perception and memory aggregates without a detail DTO.
    ///
    /// Headless controllers call this without constructing diagnostic DTOs.
    pub fn record_sensorium_metrics(
        &mut self,
        perceived_entities: u16,
        memory_counts: &[(MemoryKind, MemoryStatus, u32)],
    ) {
        metrics::histogram!("blackflower_agent_perceived_entities")
            .record(f64::from(perceived_entities));
        let mut next = [0_u32; MEMORY_METRIC_SERIES];
        for (kind, status, count) in memory_counts {
            let slot = memory_slot(*kind, *status);
            next[slot] = next[slot].saturating_add(*count);
        }
        self.replace_memory_counts(next);
    }

    /// Emit one exact completed controller decision and its aggregate metrics.
    pub fn record_decision(&mut self, record: DecisionRecord) -> Result<(), AgentDiagnosticError> {
        self.ensure_agent(record.agent_id())?;
        self.record_decision_metrics(
            record.source(),
            record.outcome(),
            record.decision_duration(),
            record.inference_duration(),
            record.budget_exhausted(),
            record.fallback_reason(),
        )?;
        self.publish(AgentDiagnosticRecord::Decision(record));
        Ok(())
    }

    /// Record aggregate controller decision telemetry without a detail record.
    #[allow(
        clippy::too_many_arguments,
        reason = "the metric call mirrors the bounded decision outcome without allocating a DTO"
    )]
    pub fn record_decision_metrics(
        &self,
        source: PolicySource,
        outcome: DecisionOutcome,
        decision_duration: Duration,
        inference_duration: Option<Duration>,
        budget_exhausted: bool,
        fallback_reason: Option<FallbackReason>,
    ) -> Result<(), AgentDiagnosticError> {
        if outcome == DecisionOutcome::Fallback && fallback_reason.is_none() {
            return Err(AgentDiagnosticError::MissingFallbackReason);
        }
        metrics::counter!(
            "blackflower_agent_decisions_total",
            "source" => source.as_str(),
            "outcome" => outcome.as_str(),
        )
        .increment(1);
        metrics::histogram!(
            "blackflower_agent_decision_duration_seconds",
            "source" => source.as_str(),
        )
        .record(decision_duration.as_secs_f64());
        if let Some(duration) = inference_duration {
            metrics::histogram!(
                "blackflower_agent_inference_duration_seconds",
                "outcome" => outcome.as_str(),
            )
            .record(duration.as_secs_f64());
        }
        if budget_exhausted {
            metrics::counter!("blackflower_agent_decision_budget_exhaustions_total").increment(1);
        }
        if let Some(reason) = fallback_reason {
            metrics::counter!(
                "blackflower_agent_fallbacks_total",
                "reason" => reason.as_str(),
            )
            .increment(1);
        }
        Ok(())
    }

    /// Record one bounded navigation-worker query measurement.
    pub fn record_navigation_query(&self, result: NavigationQueryResult, duration: Duration) {
        metrics::histogram!(
            "blackflower_agent_navigation_query_duration_seconds",
            "result" => result.as_str(),
        )
        .record(duration.as_secs_f64());
    }

    /// Record a semantic-memory eviction performed by the real memory store.
    pub fn record_memory_eviction(&self, reason: MemoryEvictionReason) {
        metrics::counter!(
            "blackflower_agent_memory_evictions_total",
            "reason" => reason.as_str(),
        )
        .increment(1);
    }

    fn ensure_agent(&self, actual: AgentId) -> Result<(), AgentDiagnosticError> {
        if let Some(descriptor) = &self.descriptor
            && descriptor.id() != actual
        {
            return Err(AgentDiagnosticError::AgentIdentityMismatch {
                expected: descriptor.id(),
                actual,
            });
        }
        Ok(())
    }

    fn replace_health(&mut self, health: AgentHealth) {
        metrics::gauge!("blackflower_agent_agents", "health" => self.health.as_str())
            .decrement(1.0);
        metrics::gauge!("blackflower_agent_agents", "health" => health.as_str()).increment(1.0);
        self.health = health;
    }

    fn replace_memory_counts(&mut self, next: [u32; MEMORY_METRIC_SERIES]) {
        for (slot, (old_count, next_count)) in
            self.memory_counts.iter().zip(next.iter()).enumerate()
        {
            let delta = next_count.cast_signed() - old_count.cast_signed();
            if delta != 0 {
                adjust_memory_gauge(memory_identity(slot), delta);
            }
        }
        self.memory_counts = next;
    }

    fn publish_status(&mut self) {
        if self.sender.is_none() {
            return;
        }
        let Some(descriptor) = self.descriptor.clone() else {
            return;
        };
        self.publish(AgentDiagnosticRecord::Status(AgentStatusSnapshot {
            descriptor,
            session_state: self.session_state,
            health: self.health,
            recorded_at: Instant::now(),
        }));
    }

    fn publish(&mut self, record: AgentDiagnosticRecord) {
        let kind = record.kind();
        let Some(sender) = self.sender.as_ref() else {
            return;
        };
        match sender.0.try_send(record) {
            Ok(()) => {
                if self.saturated.remove(&kind) {
                    tracing::info!(
                        target: "blackflower_agent",
                        event_name = "agent_diagnostic_queue_recovered",
                        kind = kind.as_str(),
                        "agent diagnostic queue recovered",
                    );
                }
            }
            Err(TrySendError::Full(_record)) => {
                metrics::counter!(
                    "blackflower_agent_diagnostic_records_dropped_total",
                    "kind" => kind.as_str(),
                )
                .increment(1);
                if self.saturated.insert(kind) {
                    tracing::warn!(
                        target: "blackflower_agent",
                        event_name = "agent_diagnostic_queue_saturated",
                        kind = kind.as_str(),
                        "agent diagnostic queue saturated",
                    );
                }
            }
            Err(TrySendError::Disconnected(_record)) => {
                self.sender = None;
                self.saturated.clear();
                tracing::info!(
                    target: "blackflower_agent",
                    event_name = "agent_diagnostic_observer_detached",
                    "agent diagnostic observer detached",
                );
            }
        }
    }
}

impl Drop for AgentDiagnostics {
    fn drop(&mut self) {
        self.session_state = SessionState::Closing;
        self.publish_status();
        for (slot, count) in self.memory_counts.iter().enumerate() {
            if *count != 0 {
                adjust_memory_gauge(memory_identity(slot), -count.cast_signed());
            }
        }
        metrics::gauge!("blackflower_agent_agents", "health" => self.health.as_str())
            .decrement(1.0);
        metrics::gauge!("blackflower_agent_active_agents").decrement(1.0);
        tracing::info!(
            target: "blackflower_agent",
            event_name = "agent_runtime_stopped",
            "agent runtime stopped",
        );
    }
}

/// Stable bounded result of one navigation-worker query.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
#[non_exhaustive]
pub enum NavigationQueryResult {
    /// A complete route was produced.
    Complete,
    /// A bounded partial route was produced explicitly.
    Partial,
    /// No legal route exists under the cooked filter.
    NoPath,
    /// Query validation or native execution failed.
    Failed,
}

impl NavigationQueryResult {
    /// Stable low-cardinality metric value.
    #[must_use]
    pub const fn as_str(self) -> &'static str {
        match self {
            Self::Complete => "complete",
            Self::Partial => "partial",
            Self::NoPath => "no_path",
            Self::Failed => "failed",
        }
    }
}

/// Stable bounded semantic-memory eviction reason.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
#[non_exhaustive]
pub enum MemoryEvictionReason {
    /// Gameplay-owned time-to-live expired.
    Expired,
    /// A bounded memory class reached capacity.
    Capacity,
    /// New legal evidence invalidated an older belief.
    Contradicted,
    /// Episode, map, control binding, or resync reset memory.
    Reset,
}

impl MemoryEvictionReason {
    /// Stable low-cardinality metric value.
    #[must_use]
    pub const fn as_str(self) -> &'static str {
        match self {
            Self::Expired => "expired",
            Self::Capacity => "capacity",
            Self::Contradicted => "contradicted",
            Self::Reset => "reset",
        }
    }
}

/// Validation failure for bounded process-local diagnostic records.
#[derive(Debug, Clone, PartialEq, Eq, thiserror::Error)]
pub enum AgentDiagnosticError {
    /// A bounded diagnostic string was empty or too large.
    #[error("diagnostic text length {actual} is outside 1..={maximum} bytes")]
    InvalidTextLength { actual: usize, maximum: usize },
    /// Terminal control characters are not valid operator-facing values.
    #[error("diagnostic text must not contain control characters")]
    InvalidTextCharacter,
    /// A bounded diagnostic collection exceeded its contract.
    #[error("{collection} has {actual} items; maximum is {maximum}")]
    CollectionTooLarge {
        collection: &'static str,
        actual: usize,
        maximum: usize,
    },
    /// Observation sequence zero is reserved for unavailable data.
    #[error("observation sequence must be non-zero")]
    ZeroObservationSequence,
    /// Decision sequence zero is reserved for unavailable data.
    #[error("decision sequence must be non-zero")]
    ZeroDecisionSequence,
    /// Sensorium schema version zero is reserved.
    #[error("sensorium schema version must be non-zero")]
    ZeroSchemaVersion,
    /// Process-local memory tokens must be non-zero.
    #[error("memory token must be non-zero")]
    ZeroMemoryToken,
    /// Confidence and uncertainty must be finite normalized values.
    #[error("confidence and uncertainty must be finite values in 0..=1")]
    InvalidNormalizedValue,
    /// A sensorium snapshot repeated one semantic channel.
    #[error("sensorium channels must be unique")]
    DuplicateSensoriumChannel,
    /// A sensorium snapshot repeated one process-local memory token.
    #[error("memory tokens must be unique within one snapshot")]
    DuplicateMemoryToken,
    /// Candidate scores must be finite.
    #[error("decision candidate score must be finite")]
    NonFiniteScore,
    /// Fallback decisions must classify their reason.
    #[error("fallback decision requires a bounded fallback reason")]
    MissingFallbackReason,
    /// A runtime observer received a record for another process-local agent.
    #[error("diagnostic agent identity mismatch: expected {expected}, got {actual}")]
    AgentIdentityMismatch { expected: AgentId, actual: AgentId },
}

/// Describe and initialize the complete low-cardinality agent metric registry.
#[allow(
    clippy::too_many_lines,
    reason = "initialization enumerates every bounded zero-valued metric series"
)]
pub fn initialize_agent_metrics() {
    describe_agent_metrics();
    metrics::gauge!("blackflower_agent_active_agents").set(0.0);
    for health in [
        AgentHealth::Starting,
        AgentHealth::Healthy,
        AgentHealth::Stalled,
        AgentHealth::Fallback,
    ] {
        metrics::gauge!("blackflower_agent_agents", "health" => health.as_str()).set(0.0);
    }
    for source in [
        PolicySource::Classical,
        PolicySource::NeuralNetwork,
        PolicySource::Held,
        PolicySource::Fallback,
    ] {
        for outcome in [
            DecisionOutcome::Completed,
            DecisionOutcome::Held,
            DecisionOutcome::Rejected,
            DecisionOutcome::Fallback,
        ] {
            metrics::counter!(
                "blackflower_agent_decisions_total",
                "source" => source.as_str(),
                "outcome" => outcome.as_str(),
            )
            .increment(0);
        }
    }
    for kind in [
        DiagnosticRecordKind::Status,
        DiagnosticRecordKind::Sensorium,
        DiagnosticRecordKind::Decision,
    ] {
        metrics::counter!(
            "blackflower_agent_diagnostic_records_dropped_total",
            "kind" => kind.as_str(),
        )
        .increment(0);
    }
    for reason in [
        FallbackReason::Budget,
        FallbackReason::Policy,
        FallbackReason::Navigation,
        FallbackReason::InvalidAction,
        FallbackReason::Session,
    ] {
        metrics::counter!(
            "blackflower_agent_fallbacks_total",
            "reason" => reason.as_str(),
        )
        .increment(0);
    }
    for reason in [
        MemoryEvictionReason::Expired,
        MemoryEvictionReason::Capacity,
        MemoryEvictionReason::Contradicted,
        MemoryEvictionReason::Reset,
    ] {
        metrics::counter!(
            "blackflower_agent_memory_evictions_total",
            "reason" => reason.as_str(),
        )
        .increment(0);
    }
    for kind in [
        MemoryKind::Sensory,
        MemoryKind::Spatial,
        MemoryKind::Episodic,
        MemoryKind::Working,
    ] {
        for status in [
            MemoryStatus::Observed,
            MemoryStatus::Remembered,
            MemoryStatus::Inferred,
            MemoryStatus::Expired,
        ] {
            metrics::gauge!(
                "blackflower_agent_memory_items",
                "kind" => kind.as_str(),
                "status" => status.as_str(),
            )
            .set(0.0);
        }
    }
    metrics::counter!("blackflower_agent_decision_budget_exhaustions_total").increment(0);
}

#[allow(
    clippy::too_many_lines,
    reason = "metric declarations form one auditable low-cardinality registry"
)]
fn describe_agent_metrics() {
    metrics::describe_gauge!(
        "blackflower_agent_active_agents",
        Unit::Count,
        "Current number of established ordinary-client agent runtimes",
    );
    metrics::describe_gauge!(
        "blackflower_agent_agents",
        Unit::Count,
        "Current agent runtimes by bounded health state",
    );
    metrics::describe_counter!(
        "blackflower_agent_decisions_total",
        Unit::Count,
        "Completed agent decisions by bounded policy source and outcome",
    );
    metrics::describe_histogram!(
        "blackflower_agent_decision_duration_seconds",
        Unit::Seconds,
        "End-to-end agent controller decision duration by policy source",
    );
    metrics::describe_histogram!(
        "blackflower_agent_inference_duration_seconds",
        Unit::Seconds,
        "Local neural-network inference duration by terminal decision outcome",
    );
    metrics::describe_histogram!(
        "blackflower_agent_perceived_entities",
        Unit::Count,
        "Entities admitted by the real sensory filter per observation",
    );
    metrics::describe_histogram!(
        "blackflower_agent_navigation_query_duration_seconds",
        Unit::Seconds,
        "Bounded navigation-worker query duration by result",
    );
    metrics::describe_counter!(
        "blackflower_agent_fallbacks_total",
        Unit::Count,
        "Neutral or classical agent fallbacks by bounded reason",
    );
    metrics::describe_counter!(
        "blackflower_agent_decision_budget_exhaustions_total",
        Unit::Count,
        "Agent decisions skipped or curtailed by the shared CPU budget",
    );
    metrics::describe_gauge!(
        "blackflower_agent_memory_items",
        Unit::Count,
        "Current aggregate semantic-memory occupancy by kind and status",
    );
    metrics::describe_counter!(
        "blackflower_agent_memory_evictions_total",
        Unit::Count,
        "Semantic-memory evictions by bounded reason",
    );
    metrics::describe_counter!(
        "blackflower_agent_diagnostic_records_dropped_total",
        Unit::Count,
        "Foreground diagnostic records dropped by a full bounded queue",
    );
}

fn adjust_memory_gauge(identity: (MemoryKind, MemoryStatus), delta: i32) {
    let gauge = metrics::gauge!(
        "blackflower_agent_memory_items",
        "kind" => identity.0.as_str(),
        "status" => identity.1.as_str(),
    );
    if delta >= 0 {
        gauge.increment(f64::from(delta));
    } else {
        gauge.decrement(f64::from(delta.unsigned_abs()));
    }
}

const fn memory_slot(kind: MemoryKind, status: MemoryStatus) -> usize {
    kind.index() * MEMORY_STATUS_COUNT + status.index()
}

fn memory_identity(slot: usize) -> (MemoryKind, MemoryStatus) {
    (
        MEMORY_KINDS[slot / MEMORY_STATUS_COUNT],
        MEMORY_STATUSES[slot % MEMORY_STATUS_COUNT],
    )
}

fn is_normalized(value: f32) -> bool {
    value.is_finite() && (0.0..=1.0).contains(&value)
}

#[cfg(test)]
#[path = "../tests/unit/diagnostics.rs"]
mod tests;
