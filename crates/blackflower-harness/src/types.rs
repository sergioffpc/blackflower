use std::net::SocketAddr;
use std::num::NonZeroU64;

use blackflower_networking::{
    AdmissionRejectReason, CommandDisposition, CommandId, CommandTimingClass,
    CompatibilityContract, ConnectionEpoch, SessionState, SimulationTick,
};
use blackflower_networking_replication::Snapshot;

use crate::{PredictionUpdate, SnapshotWindow};

/// Immutable construction parameters shared by human and headless clients.
#[derive(Debug, Clone, PartialEq, Eq)]
pub struct ClientHarnessConfig {
    /// Exact protocol, simulation, and cooked-content contract.
    pub compatibility: CompatibilityContract,
    /// Generation of the already established transport connection.
    pub connection_epoch: ConnectionEpoch,
    /// Short-lived one-use admission ticket.
    pub admission_ticket: Vec<u8>,
}

/// Current server-authorized object controlled by this client session.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub struct ControlBinding {
    /// Generation incremented whenever the controlled object changes.
    pub control_epoch: u32,
    /// Non-zero replicated identity owned by the session.
    pub controlled_entity: NonZeroU64,
}

/// One source-neutral canonical control submission.
#[derive(Debug, Clone, PartialEq, Eq)]
pub struct ControlSubmission {
    /// First authoritative tick covered by this four-tick control frame.
    pub execute_tick: SimulationTick,
    /// Opaque canonical gameplay control bytes.
    pub payload: Vec<u8>,
    /// Discrete commands originating from this control frame.
    pub commands: Vec<CommandSubmission>,
}

/// One gameplay command before the harness assigns stable idempotency identities.
#[derive(Debug, Clone, PartialEq, Eq)]
pub struct CommandSubmission {
    /// Requested authoritative execution tick.
    pub execute_tick: SimulationTick,
    /// Historical client view used by rewind-capable commands.
    pub view_tick: Option<SimulationTick>,
    /// Network timing and historical execution policy.
    pub timing_class: CommandTimingClass,
    /// Revision-registered gameplay command kind.
    pub kind: u16,
    /// Opaque canonical gameplay command bytes.
    pub payload: Vec<u8>,
}

/// Client-facing fact emitted after session, replication, or prediction work.
#[derive(Debug, Clone, PartialEq, Eq)]
pub enum ClientEvent {
    /// Admission was rejected without activating the client.
    AdmissionRejected(AdmissionRejectReason),
    /// A replacement reconnect token was issued.
    ResumeIssued {
        /// Opaque one-use token bytes.
        token: Vec<u8>,
        /// Remaining lifetime from issuance.
        expires_in_millis: u32,
    },
    /// Server disposition for one idempotent command.
    CommandDisposition {
        /// Stable command identity assigned by the harness.
        command_id: CommandId,
        /// Current authoritative disposition.
        disposition: CommandDisposition,
    },
    /// One validated server time-sync response or unexpected request.
    TimeSync(blackflower_networking::TimeSyncMessage),
    /// An authoritative projection was applied and prediction was reconciled.
    SnapshotApplied {
        /// Applied authoritative tick.
        tick: SimulationTick,
        /// Result of coordinating the prediction timeline.
        prediction: PredictionUpdate,
    },
    /// The synchronized session reached its scheduled activation tick.
    Activated { tick: SimulationTick },
    /// One exact voice-delivery datagram for the audio or bot event consumer.
    VoiceDatagram(Vec<u8>),
    /// The validated peer path changed.
    PathChanged {
        /// Previous peer address.
        previous: SocketAddr,
        /// New peer address.
        current: SocketAddr,
    },
    /// The low-level transport stopped.
    TransportStopped,
    /// The server closed the application session.
    Closing { code: u16 },
}

/// Read-only state consumed by presentation or bot decision code.
#[derive(Debug, Clone, Copy)]
pub struct ClientView<'a, S> {
    pub(crate) session_state: SessionState,
    pub(crate) authoritative: SnapshotWindow<'a>,
    pub(crate) predicted: Option<&'a S>,
    pub(crate) pending_events: usize,
}

impl<S> ClientView<'_, S> {
    /// Return the normative application-session lifecycle state.
    #[must_use]
    pub const fn session_state(&self) -> SessionState {
        self.session_state
    }

    /// Return the latest fully reconstructed authoritative projection.
    #[must_use]
    pub fn authoritative(&self) -> Option<&Snapshot> {
        self.authoritative.newest()
    }

    /// Return the bounded chronological window used for remote interpolation.
    #[must_use]
    pub const fn authoritative_window(&self) -> SnapshotWindow<'_> {
        self.authoritative
    }

    /// Return the latest locally sealed predicted state.
    #[must_use]
    pub const fn predicted(&self) -> Option<&S> {
        self.predicted
    }

    /// Return the number of client-facing events waiting to be consumed.
    #[must_use]
    pub const fn pending_events(&self) -> usize {
        self.pending_events
    }
}
