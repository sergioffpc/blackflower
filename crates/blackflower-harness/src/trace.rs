use blackflower_networking::{InputSequence, SessionState};
use blackflower_networking_replication::Snapshot;

use crate::{ControlSubmission, SnapshotWindow};

/// Boundary observation-and-action record handed to a [`TraceObserver`] every
/// time a client submits input through the harness.
///
/// The record carries only what a client can legitimately perceive: the
/// authoritative projection window it has reconstructed and the canonical input
/// it just submitted. It deliberately excludes predicted, server-only, and ECS
/// state so that a recorded corpus matches, field for field, exactly what a bot
/// observes at inference time. Recording at this boundary is the guarantee that
/// an imitation-learning dataset cannot teach the bot to use information a fair
/// player never had (see `docs/bots-v1.md`, BOT-DATA and BOT-TRACE).
///
/// A record borrows harness-internal state and must not be retained beyond the
/// [`TraceObserver::on_control_submitted`] call; a sink that needs to keep data
/// must copy what it needs.
#[derive(Debug, Clone, Copy)]
pub struct TraceRecord<'a> {
    /// Application-session lifecycle state at submission time.
    pub session_state: SessionState,
    /// Stable input sequence assigned to this accepted submission.
    pub input_sequence: InputSequence,
    /// Newest fully reconstructed authoritative projection, if any exists yet.
    ///
    /// This is the world view the client acted upon and the primary source from
    /// which a perception encoder derives its feature vector offline.
    pub authoritative: Option<&'a Snapshot>,
    /// Bounded chronological window used for interpolation and velocity
    /// derivation. Retaining the window lets an offline encoder reconstruct
    /// motion the same way live perception does.
    pub window: SnapshotWindow<'a>,
    /// Canonical control the client submitted: the imitation-learning label.
    pub submission: &'a ControlSubmission,
}

/// Sink for boundary (observation, action) pairs captured at the harness input
/// choke point.
///
/// The harness owns no I/O and installs no sink by default; a caller attaches
/// one with [`ClientHarness::set_trace_observer`](crate::ClientHarness::set_trace_observer)
/// only for consenting human-capture sessions. The sink is invoked once per
/// accepted submission, after the input has been validated, sequenced, queued
/// for prediction, and published to transport, so it never observes rejected or
/// partially applied input.
///
/// Implementations serialize or forward records; the concrete recorder
/// (framing, compression, file rotation) lives outside this crate to keep the
/// harness dependency-light.
pub trait TraceObserver: Send {
    /// Handle one accepted client submission together with the perception the
    /// client held when it produced that submission.
    fn on_control_submitted(&mut self, record: TraceRecord<'_>);
}
