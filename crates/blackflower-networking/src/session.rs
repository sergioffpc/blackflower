use std::collections::VecDeque;
use std::time::Duration;

use crate::{
    AdmissionClaims, ConnectionEpoch, ProtocolRevision, RequiredContentSetId,
    SimulationCompatibilityId, SimulationTick,
};

/// Minimum activation lead after admission, in simulation ticks.
pub const MINIMUM_ACTIVATION_LEAD_TICKS: u64 = 24;
/// Activation ticks are aligned to this quantum.
pub const ACTIVATION_ALIGNMENT_TICKS: u64 = 4;
/// Maximum post-activation full resynchronization in the rolling window.
pub const MAX_RESYNCS_PER_WINDOW: usize = 2;
/// Rolling interval used for post-activation resynchronization limits.
pub const RESYNC_WINDOW: Duration = Duration::from_secs(60);

/// Normative application session lifecycle above a QUIC connection.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum SessionState {
    /// QUIC connection establishment is in progress.
    Connecting,
    /// TLS 1.3 and ALPN are established.
    Secure,
    /// A ticket or resume token is being consumed.
    Authenticating,
    /// Exact protocol, simulation, and content identities match.
    Compatible,
    /// Clock and full-state bootstrap are being established.
    Synchronizing,
    /// Inputs and incremental snapshots may affect the game session.
    Active,
    /// A bounded full snapshot and new activation are required.
    Resynchronizing,
    /// The application session is closing and cannot reactivate.
    Closing,
}

/// Operational degradation independent of lifecycle state.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum OperationalState {
    /// All time and traffic gates are within their nominal bounds.
    Nominal,
    /// Clock uncertainty exceeded four ticks for three samples.
    ClockDegraded,
    /// Applied snapshot progress has stopped for at least 500 ms.
    SnapshotStalled,
    /// Canonical input has stopped for at least one second.
    InputFailsafe,
    /// No authenticated application traffic was seen for five seconds.
    ConnectionUnresponsive,
}

/// Exact compatibility values required by one server deployment.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub struct CompatibilityContract {
    /// Required application protocol revision.
    pub protocol_revision: ProtocolRevision,
    /// Required deterministic simulation identity.
    pub simulation_compatibility_id: SimulationCompatibilityId,
    /// Required cooked-content set identity.
    pub required_content_set_id: RequiredContentSetId,
}

/// Invalid application-session transition or admission claim.
#[derive(Debug, Clone, Copy, PartialEq, Eq, thiserror::Error)]
pub enum SessionError {
    /// The event is not legal from the current lifecycle state.
    #[error("invalid session transition from {from:?} to {to:?}")]
    InvalidTransition {
        /// Current lifecycle state.
        from: SessionState,
        /// Requested lifecycle state.
        to: SessionState,
    },
    /// One or more exact compatibility identities differ.
    #[error("admission claims are incompatible")]
    Incompatible,
    /// Activation tick does not meet alignment or minimum lead.
    #[error("activation tick is invalid")]
    InvalidActivation,
    /// The rolling resynchronization limit has been exhausted.
    #[error("resynchronization rate limit exceeded")]
    ResyncRateLimited,
    /// A reconnect did not advance the connection generation.
    #[error("connection epoch did not advance")]
    StaleConnectionEpoch,
}

/// Deterministic client-side application session machine.
#[derive(Debug, Clone)]
pub struct ClientSession {
    core: SessionCore,
}

/// Deterministic server-side application session machine.
#[derive(Debug, Clone)]
pub struct ServerSession {
    core: SessionCore,
}

/// Determine the operational state from monotonic progress timestamps.
#[must_use]
pub fn operational_state(
    now: Duration,
    last_authenticated_traffic: Duration,
    last_snapshot_applied: Duration,
    last_input: Duration,
    clock_degraded: bool,
) -> OperationalState {
    if elapsed_at_least(now, last_authenticated_traffic, Duration::from_secs(5)) {
        OperationalState::ConnectionUnresponsive
    } else if elapsed_at_least(now, last_input, Duration::from_secs(1)) {
        OperationalState::InputFailsafe
    } else if elapsed_at_least(now, last_snapshot_applied, Duration::from_millis(500)) {
        OperationalState::SnapshotStalled
    } else if clock_degraded {
        OperationalState::ClockDegraded
    } else {
        OperationalState::Nominal
    }
}

/// Select a future four-tick-aligned activation tick.
#[must_use]
pub fn activation_tick(current: SimulationTick, uncertainty_ticks: u64) -> SimulationTick {
    let unaligned = current
        .get()
        .saturating_add(MINIMUM_ACTIVATION_LEAD_TICKS)
        .saturating_add(uncertainty_ticks);
    SimulationTick::new(align_up(unaligned, ACTIVATION_ALIGNMENT_TICKS))
}

macro_rules! session_api {
    ($type_name:ident) => {
        impl $type_name {
            /// Create a session before TLS establishment.
            #[must_use]
            pub fn new(contract: CompatibilityContract, connection_epoch: ConnectionEpoch) -> Self {
                Self {
                    core: SessionCore::new(contract, connection_epoch),
                }
            }

            /// Return the current normative lifecycle state.
            #[must_use]
            pub const fn state(&self) -> SessionState {
                self.core.state
            }

            /// Return the connection generation accepted by this session.
            #[must_use]
            pub const fn connection_epoch(&self) -> ConnectionEpoch {
                self.core.connection_epoch
            }

            /// Record successful TLS 1.3 and ALPN establishment.
            pub fn secure(&mut self) -> Result<(), SessionError> {
                self.core.transition(SessionState::Secure)
            }

            /// Begin atomic ticket or resume-token consumption.
            pub fn authenticate(&mut self) -> Result<(), SessionError> {
                self.core.transition(SessionState::Authenticating)
            }

            /// Check exact identities and record compatibility.
            pub fn accept_claims(&mut self, claims: &AdmissionClaims) -> Result<(), SessionError> {
                self.core.accept_claims(claims)
            }

            /// Begin clock synchronization and full-state bootstrap.
            pub fn synchronize(&mut self) -> Result<(), SessionError> {
                self.core.transition(SessionState::Synchronizing)
            }

            /// Validate and retain a future activation while still synchronizing.
            pub fn schedule_activation(
                &mut self,
                current: SimulationTick,
                scheduled: SimulationTick,
            ) -> Result<(), SessionError> {
                self.core.schedule_activation(current, scheduled)
            }

            /// Advance to `Active` only when the scheduled tick is reached.
            pub fn advance(&mut self, current: SimulationTick) -> Result<bool, SessionError> {
                self.core.advance(current)
            }

            /// Return the pending activation tick, if synchronization completed.
            #[must_use]
            pub const fn scheduled_activation(&self) -> Option<SimulationTick> {
                self.core.scheduled_activation
            }

            /// Enter a bounded full-state resynchronization.
            pub fn begin_resync(&mut self, now: Duration) -> Result<(), SessionError> {
                self.core.begin_resync(now)
            }

            /// Return to synchronization after a reconnect with a fresh epoch.
            pub fn reconnect(&mut self, epoch: ConnectionEpoch) -> Result<(), SessionError> {
                self.core.reconnect(epoch)
            }

            /// Permanently enter closing state.
            pub fn close(&mut self) -> Result<(), SessionError> {
                self.core.transition(SessionState::Closing)
            }
        }
    };
}

session_api!(ClientSession);
session_api!(ServerSession);

#[derive(Debug, Clone)]
struct SessionCore {
    state: SessionState,
    contract: CompatibilityContract,
    connection_epoch: ConnectionEpoch,
    resyncs: VecDeque<Duration>,
    scheduled_activation: Option<SimulationTick>,
}

impl SessionCore {
    fn new(contract: CompatibilityContract, connection_epoch: ConnectionEpoch) -> Self {
        Self {
            state: SessionState::Connecting,
            contract,
            connection_epoch,
            resyncs: VecDeque::with_capacity(MAX_RESYNCS_PER_WINDOW),
            scheduled_activation: None,
        }
    }

    fn transition(&mut self, target: SessionState) -> Result<(), SessionError> {
        if !valid_transition(self.state, target) {
            return Err(SessionError::InvalidTransition {
                from: self.state,
                to: target,
            });
        }
        self.state = target;
        Ok(())
    }

    fn accept_claims(&mut self, claims: &AdmissionClaims) -> Result<(), SessionError> {
        if self.state != SessionState::Authenticating {
            return self.transition(SessionState::Compatible);
        }
        if !self.contract.matches(claims) {
            return Err(SessionError::Incompatible);
        }
        self.state = SessionState::Compatible;
        Ok(())
    }

    fn schedule_activation(
        &mut self,
        current: SimulationTick,
        scheduled: SimulationTick,
    ) -> Result<(), SessionError> {
        if self.state != SessionState::Synchronizing {
            return Err(SessionError::InvalidTransition {
                from: self.state,
                to: SessionState::Active,
            });
        }
        let lead = scheduled.get().saturating_sub(current.get());
        if !scheduled.get().is_multiple_of(ACTIVATION_ALIGNMENT_TICKS)
            || lead < MINIMUM_ACTIVATION_LEAD_TICKS
        {
            return Err(SessionError::InvalidActivation);
        }
        self.scheduled_activation = Some(scheduled);
        Ok(())
    }

    fn advance(&mut self, current: SimulationTick) -> Result<bool, SessionError> {
        let scheduled = self
            .scheduled_activation
            .ok_or(SessionError::InvalidActivation)?;
        if current < scheduled {
            return Ok(false);
        }
        self.transition(SessionState::Active)?;
        self.scheduled_activation = None;
        Ok(true)
    }

    fn begin_resync(&mut self, now: Duration) -> Result<(), SessionError> {
        if self.state != SessionState::Active {
            return self.transition(SessionState::Resynchronizing);
        }
        prune_resyncs(&mut self.resyncs, now);
        if self.resyncs.len() >= MAX_RESYNCS_PER_WINDOW {
            return Err(SessionError::ResyncRateLimited);
        }
        self.resyncs.push_back(now);
        self.state = SessionState::Resynchronizing;
        Ok(())
    }

    fn reconnect(&mut self, epoch: ConnectionEpoch) -> Result<(), SessionError> {
        if epoch.get() <= self.connection_epoch.get() {
            return Err(SessionError::StaleConnectionEpoch);
        }
        self.connection_epoch = epoch;
        self.state = SessionState::Synchronizing;
        self.scheduled_activation = None;
        Ok(())
    }
}

impl CompatibilityContract {
    fn matches(self, claims: &AdmissionClaims) -> bool {
        self.protocol_revision == claims.protocol_revision
            && self.simulation_compatibility_id == claims.simulation_compatibility_id
            && self.required_content_set_id == claims.required_content_set_id
    }
}

fn valid_transition(from: SessionState, to: SessionState) -> bool {
    matches!(
        (from, to),
        (SessionState::Connecting, SessionState::Secure)
            | (SessionState::Secure, SessionState::Authenticating)
            | (SessionState::Authenticating, SessionState::Compatible)
            | (SessionState::Compatible, SessionState::Synchronizing)
            | (SessionState::Synchronizing, SessionState::Active)
            | (SessionState::Resynchronizing, SessionState::Synchronizing)
            | (_, SessionState::Closing)
    )
}

fn prune_resyncs(resyncs: &mut VecDeque<Duration>, now: Duration) {
    while resyncs.front().is_some_and(|then| {
        now.checked_sub(*then)
            .is_some_and(|elapsed| elapsed >= RESYNC_WINDOW)
    }) {
        let _removed = resyncs.pop_front();
    }
}

fn elapsed_at_least(now: Duration, then: Duration, threshold: Duration) -> bool {
    now.checked_sub(then)
        .is_some_and(|elapsed| elapsed >= threshold)
}

const fn align_up(value: u64, quantum: u64) -> u64 {
    value.saturating_add(quantum - 1) / quantum * quantum
}
