use std::collections::{BTreeMap, VecDeque};

use bytes::Bytes;

use crate::{
    CommandId, CommandTimingClass, ControlFrame, DiscreteCommand, InputSequence, ProtocolRevision,
    SimulationTick, WireError,
};

/// Canonical input and prediction history retained by networking.
pub const INPUT_HISTORY_TICKS: u64 = 512;
/// Maximum client reconciliation rollback.
pub const MAX_ROLLBACK_TICKS: u64 = 64;
/// Maximum read-only hitscan rewind.
pub const MAX_REWIND_RAY_TICKS: u64 = 32;
/// Maximum projectile catch-up interval.
pub const MAX_CATCH_UP_BALLISTIC_TICKS: u64 = 16;
/// Maximum future execution lead accepted from a client.
pub const MAX_FUTURE_COMMAND_TICKS: u64 = 24;
/// Last canonical control is held for this many missing ticks.
pub const INPUT_GRACE_TICKS: u64 = 12;
/// Missing input becomes a connection-level failsafe at this age.
pub const INPUT_FAILSAFE_TICKS: u64 = 240;

/// Revision-specific validator for canonical control bytes.
///
/// Networking owns bounds and identity. Gameplay owns the byte schema and must
/// register exactly one implementation for each supported protocol revision.
pub trait ControlCodec {
    /// Exact protocol revision understood by this codec.
    fn protocol_revision(&self) -> ProtocolRevision;

    /// Validate canonical control bytes without changing gameplay state.
    fn validate_control(&self, bytes: &[u8]) -> Result<(), CodecViolation>;
}

/// Revision-specific validator for opaque discrete command payloads.
pub trait CommandCodec {
    /// Exact protocol revision understood by this codec.
    fn protocol_revision(&self) -> ProtocolRevision;

    /// Validate a registered command kind and its canonical payload.
    fn validate_command(&self, kind: u16, bytes: &[u8]) -> Result<(), CodecViolation>;
}

/// A registered gameplay codec rejected structurally invalid canonical bytes.
#[derive(Debug, Clone, Copy, PartialEq, Eq, thiserror::Error)]
pub enum CodecViolation {
    /// The kind is not registered for this protocol revision.
    #[error("unregistered gameplay codec kind")]
    UnknownKind,
    /// Bytes are not canonical for the registered gameplay schema.
    #[error("non-canonical gameplay codec payload")]
    NonCanonical,
}

/// Result of observing an idempotency identity.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum Deduplication {
    /// Identity was not previously present in retained history.
    New,
    /// Identity and every canonical byte match the retained value.
    Duplicate,
    /// Identity predates the newest accepted control and is no longer retained.
    Stale,
}

/// Conflicting use of an idempotency identity is a protocol violation.
#[derive(Debug, Clone, Copy, PartialEq, Eq, thiserror::Error)]
pub enum DeduplicationError {
    /// Same input sequence was reused with different canonical content.
    #[error("input sequence was reused with different bytes")]
    ConflictingInput(InputSequence),
    /// Same command identity was reused with different canonical content.
    #[error("command identity was reused with different bytes")]
    ConflictingCommand(CommandId),
}

/// Bounded exact-byte history for redundant input and command delivery.
#[derive(Debug, Clone)]
pub struct InputDeduplicator {
    controls: BTreeMap<InputSequence, ControlIdentity>,
    control_order: VecDeque<InputSequence>,
    newest_control: Option<InputSequence>,
    commands: BTreeMap<CommandId, CommandIdentity>,
    command_order: VecDeque<CommandId>,
    capacity: usize,
}

impl Default for InputDeduplicator {
    fn default() -> Self {
        Self::new(usize::try_from(INPUT_HISTORY_TICKS).unwrap_or(usize::MAX))
    }
}

impl InputDeduplicator {
    /// Create histories with an explicit deterministic identity bound.
    #[must_use]
    pub fn new(capacity: usize) -> Self {
        Self {
            controls: BTreeMap::new(),
            control_order: VecDeque::new(),
            newest_control: None,
            commands: BTreeMap::new(),
            command_order: VecDeque::new(),
            capacity,
        }
    }

    /// Observe a control frame and reject conflicting identity reuse.
    pub fn observe_control(
        &mut self,
        frame: &ControlFrame,
    ) -> Result<Deduplication, DeduplicationError> {
        let identity = ControlIdentity {
            execute_tick: frame.execute_tick,
            payload: frame.payload.clone(),
        };
        match self.controls.get(&frame.sequence) {
            Some(retained) if retained == &identity => Ok(Deduplication::Duplicate),
            Some(_retained) => Err(DeduplicationError::ConflictingInput(frame.sequence)),
            None if self
                .newest_control
                .is_some_and(|newest| frame.sequence <= newest) =>
            {
                Ok(Deduplication::Stale)
            }
            None => {
                self.controls.insert(frame.sequence, identity);
                self.control_order.push_back(frame.sequence);
                self.newest_control = Some(frame.sequence);
                evict_oldest(&mut self.controls, &mut self.control_order, self.capacity);
                Ok(Deduplication::New)
            }
        }
    }

    /// Observe a discrete command and reject conflicting identity reuse.
    pub fn observe_command(
        &mut self,
        command: &DiscreteCommand,
    ) -> Result<Deduplication, DeduplicationError> {
        let identity = CommandIdentity::from(command);
        match self.commands.get(&command.command_id) {
            Some(retained) if retained == &identity => Ok(Deduplication::Duplicate),
            Some(_retained) => Err(DeduplicationError::ConflictingCommand(command.command_id)),
            None => {
                self.commands.insert(command.command_id, identity);
                self.command_order.push_back(command.command_id);
                evict_oldest(&mut self.commands, &mut self.command_order, self.capacity);
                Ok(Deduplication::New)
            }
        }
    }
}

/// Network-level reason a discrete command cannot be queued.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum CommandRejection {
    /// Requested tick is more than twenty-four ticks in the future.
    TooFarInFuture,
    /// Requested tick falls outside the timing class's late window.
    TooLate,
    /// Historical timing class omitted or exceeded its view tick.
    InvalidHistoricalTick,
    /// Temporal command execution is blocked by clock safety.
    ClockUnsafe,
}

/// Network timing classification of a validated discrete command.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum CommandTimingDecision {
    /// Command may be delivered to gameplay at this authoritative tick.
    Deliver {
        /// Effective execution tick selected by networking.
        effective_tick: SimulationTick,
        /// Optional immutable history tick for a historical query.
        historical_tick: Option<SimulationTick>,
    },
    /// Command is rejected before gameplay dispatch.
    Reject(CommandRejection),
}

/// Classify one command without executing gameplay.
#[must_use]
pub fn classify_command(
    now: SimulationTick,
    command: &DiscreteCommand,
    clock_safe: bool,
) -> CommandTimingDecision {
    if !clock_safe && is_temporal(command.timing_class) {
        return CommandTimingDecision::Reject(CommandRejection::ClockUnsafe);
    }
    if command.execute_tick.get() > now.get().saturating_add(MAX_FUTURE_COMMAND_TICKS) {
        return CommandTimingDecision::Reject(CommandRejection::TooFarInFuture);
    }
    let lateness = now.get().saturating_sub(command.execute_tick.get());
    let window = late_window(command.timing_class);
    if lateness > window {
        return CommandTimingDecision::Reject(CommandRejection::TooLate);
    }
    let historical_tick = match historical_tick(now, command) {
        Ok(tick) => tick,
        Err(rejection) => return CommandTimingDecision::Reject(rejection),
    };
    CommandTimingDecision::Deliver {
        effective_tick: command.execute_tick,
        historical_tick,
    }
}

/// Read-only history exposed while dispatching historical command classes.
pub trait HistoricalCommandContext {
    /// Read a hitscan-relevant projection no more than thirty-two ticks old.
    fn rewind_ray(&self, tick: SimulationTick) -> Option<&[u8]>;

    /// Read projectile catch-up state no more than sixteen ticks old.
    fn catch_up_ballistic(&self, tick: SimulationTick) -> Option<&[u8]>;
}

/// Current status derived from the age of the last canonical input frame.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum InputHealth {
    /// Continue holding the last canonical control frame.
    Holding,
    /// Emit neutral control and release held edges.
    Neutralized,
    /// Connection-level input failsafe has expired.
    Failsafe,
}

/// Classify missing input independently from authenticated traffic timeout.
#[must_use]
pub fn input_health(now: SimulationTick, last_input: SimulationTick) -> InputHealth {
    let age = now.get().saturating_sub(last_input.get());
    if age >= INPUT_FAILSAFE_TICKS {
        InputHealth::Failsafe
    } else if age > INPUT_GRACE_TICKS {
        InputHealth::Neutralized
    } else {
        InputHealth::Holding
    }
}

/// Validate codec revision and canonical bytes for one control frame.
pub fn validate_control_codec(
    revision: ProtocolRevision,
    codec: &dyn ControlCodec,
    frame: &ControlFrame,
) -> Result<(), WireError> {
    if codec.protocol_revision() != revision {
        return Err(WireError::InvalidValue("control codec revision"));
    }
    codec
        .validate_control(&frame.payload)
        .map_err(|_error| WireError::InvalidValue("control codec payload"))
}

/// Validate codec revision, kind, and canonical bytes for one command.
pub fn validate_command_codec(
    revision: ProtocolRevision,
    codec: &dyn CommandCodec,
    command: &DiscreteCommand,
) -> Result<(), WireError> {
    if codec.protocol_revision() != revision {
        return Err(WireError::InvalidValue("command codec revision"));
    }
    codec
        .validate_command(command.kind, &command.payload)
        .map_err(|_error| WireError::InvalidValue("command codec payload"))
}

#[derive(Debug, Clone, PartialEq, Eq)]
struct ControlIdentity {
    execute_tick: SimulationTick,
    payload: Bytes,
}

#[derive(Debug, Clone, PartialEq, Eq)]
struct CommandIdentity {
    origin_input_sequence: InputSequence,
    execute_tick: SimulationTick,
    view_tick: Option<SimulationTick>,
    timing_class: CommandTimingClass,
    kind: u16,
    payload: Bytes,
}

impl From<&DiscreteCommand> for CommandIdentity {
    fn from(command: &DiscreteCommand) -> Self {
        Self {
            origin_input_sequence: command.origin_input_sequence,
            execute_tick: command.execute_tick,
            view_tick: command.view_tick,
            timing_class: command.timing_class,
            kind: command.kind,
            payload: command.payload.clone(),
        }
    }
}

fn late_window(class: CommandTimingClass) -> u64 {
    match class {
        CommandTimingClass::ContinuousMovement | CommandTimingClass::Jump => 8,
        CommandTimingClass::Interaction => 12,
        CommandTimingClass::Inventory => 24,
        CommandTimingClass::RewindRay => MAX_REWIND_RAY_TICKS,
        CommandTimingClass::CatchUpBallistic => MAX_CATCH_UP_BALLISTIC_TICKS,
        CommandTimingClass::CurrentTickOnly => 0,
    }
}

fn historical_tick(
    now: SimulationTick,
    command: &DiscreteCommand,
) -> Result<Option<SimulationTick>, CommandRejection> {
    let maximum = match command.timing_class {
        CommandTimingClass::RewindRay => Some(MAX_REWIND_RAY_TICKS),
        CommandTimingClass::CatchUpBallistic => Some(MAX_CATCH_UP_BALLISTIC_TICKS),
        CommandTimingClass::ContinuousMovement
        | CommandTimingClass::Jump
        | CommandTimingClass::Interaction
        | CommandTimingClass::Inventory
        | CommandTimingClass::CurrentTickOnly => None,
    };
    let Some(maximum) = maximum else {
        return if command.view_tick.is_none() {
            Ok(None)
        } else {
            Err(CommandRejection::InvalidHistoricalTick)
        };
    };
    let tick = command
        .view_tick
        .ok_or(CommandRejection::InvalidHistoricalTick)?;
    if tick.get() > now.get() || now.get() - tick.get() > maximum {
        Err(CommandRejection::InvalidHistoricalTick)
    } else {
        Ok(Some(tick))
    }
}

const fn is_temporal(class: CommandTimingClass) -> bool {
    matches!(
        class,
        CommandTimingClass::RewindRay | CommandTimingClass::CatchUpBallistic
    )
}

fn evict_oldest<Key: Copy + Ord, Value>(
    values: &mut BTreeMap<Key, Value>,
    order: &mut VecDeque<Key>,
    capacity: usize,
) {
    while values.len() > capacity {
        let Some(oldest) = order.pop_front() else {
            break;
        };
        drop(values.remove(&oldest));
    }
}
