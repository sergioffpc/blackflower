use std::collections::VecDeque;
use std::time::Duration;

use bytes::Bytes;

use crate::voice::{MAX_AUDIBLE_VOICES, VoiceSendQueues};
use crate::{VoiceStreamId, WireError};

/// Constrained per-client upstream rate.
pub const CONSTRAINED_UPSTREAM_BITS_PER_SECOND: u64 = 128 * 1_000;
/// Preferred per-client upstream rate.
pub const PREFERRED_UPSTREAM_BITS_PER_SECOND: u64 = 256 * 1_000;
/// Constrained per-client downstream rate.
pub const CONSTRAINED_DOWNSTREAM_BITS_PER_SECOND: u64 = 512 * 1_000;
/// Preferred per-client downstream rate.
pub const PREFERRED_DOWNSTREAM_BITS_PER_SECOND: u64 = 1_024 * 1_000;
/// Constrained authoritative match egress.
pub const CONSTRAINED_MATCH_BITS_PER_SECOND: u64 = 16 * 1_000 * 1_000;
/// Preferred authoritative match egress.
pub const PREFERRED_MATCH_BITS_PER_SECOND: u64 = 32 * 1_000 * 1_000;
/// Protected minimum incremental snapshot rate.
pub const MINIMUM_SNAPSHOT_RATE_HZ: u32 = 15;
/// Maximum incremental snapshot rate under available budget.
pub const MAXIMUM_SNAPSHOT_RATE_HZ: u32 = 30;
/// Maximum queued reliable control messages.
pub const MAX_CONTROL_QUEUE_MESSAGES: usize = 64;
/// Maximum queued reliable control bytes.
pub const MAX_CONTROL_QUEUE_BYTES: usize = 1_024 * 1_024;
/// Maximum pending snapshot generations.
pub const MAX_PENDING_SNAPSHOTS: usize = 32;
/// Maximum host events pending delivery to the game host.
pub const MAX_HOST_EVENTS: usize = 128;

/// Configured link and aggregate budget tier.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum BudgetTier {
    /// Minimum supported budget under congestion.
    Constrained,
    /// Desired budget when capacity is available.
    Preferred,
}

/// Direction of application traffic for per-connection shaping.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum TrafficDirection {
    /// Client-to-server application traffic.
    Upstream,
    /// Server-to-client application traffic.
    Downstream,
}

/// Priority class selected by the deterministic outbound queue.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum TrafficClass {
    /// Reliable session control.
    SessionControl,
    /// Latest canonical input; older pending input is superseded.
    Input,
    /// Snapshot required to preserve the protected 15 Hz minimum.
    MinimumSnapshot,
    /// Live Opus packet still inside its playout deadline.
    Voice,
    /// Additional snapshot capacity up to 30 Hz.
    AdditionalSnapshot,
}

/// One application payload selected for transport.
#[derive(Debug, Clone, PartialEq, Eq)]
pub struct ScheduledPayload {
    /// Scheduling class used for observability.
    pub class: TrafficClass,
    /// Exact application bytes passed to Quinn.
    pub bytes: Bytes,
}

/// Bounded-queue backpressure result.
#[derive(Debug, Clone, PartialEq, Eq, thiserror::Error)]
pub enum QueueError {
    /// Reliable control capacity is exhausted and the session must close.
    #[error("session-control queue capacity exhausted")]
    ControlCapacity,
    /// Only one bootstrap may be active for a connection.
    #[error("state bootstrap already active")]
    BootstrapActive,
    /// Full-state bootstrap exceeds its 2 MiB bound.
    #[error(transparent)]
    Wire(#[from] WireError),
    /// More than four voice streams are pending for one receiver.
    #[error("voice queue exceeds four simultaneous streams")]
    VoiceCapacity,
    /// Host event queue is full.
    #[error("host event queue capacity exhausted")]
    HostEventCapacity,
}

/// All bounded host-facing queues for one connection.
#[derive(Debug, Default, Clone)]
pub struct NetworkQueues {
    controls: VecDeque<Bytes>,
    control_bytes: usize,
    bootstrap: Option<Bytes>,
    latest_input: Option<Bytes>,
    snapshots: VecDeque<Bytes>,
    voices: VoiceSendQueues,
    host_events: VecDeque<Bytes>,
}

impl NetworkQueues {
    /// Create empty queues with normative fixed capacities.
    #[must_use]
    pub fn new() -> Self {
        Self::default()
    }

    /// Queue reliable session control without silently dropping it.
    pub fn push_control(&mut self, bytes: Bytes) -> Result<(), QueueError> {
        let next_bytes = self.control_bytes.saturating_add(bytes.len());
        if self.controls.len() >= MAX_CONTROL_QUEUE_MESSAGES || next_bytes > MAX_CONTROL_QUEUE_BYTES
        {
            return Err(QueueError::ControlCapacity);
        }
        self.control_bytes = next_bytes;
        self.controls.push_back(bytes);
        Ok(())
    }

    /// Reserve the single bounded uncompressed bootstrap slot.
    pub fn start_bootstrap(&mut self, bytes: Bytes) -> Result<(), QueueError> {
        if self.bootstrap.is_some() {
            return Err(QueueError::BootstrapActive);
        }
        if bytes.len() > crate::MAX_BOOTSTRAP_BYTES {
            return Err(QueueError::Wire(WireError::Oversized {
                actual: bytes.len(),
                maximum: crate::MAX_BOOTSTRAP_BYTES,
            }));
        }
        self.bootstrap = Some(bytes);
        Ok(())
    }

    /// Take the active bootstrap for its dedicated unidirectional stream.
    pub fn take_bootstrap(&mut self) -> Option<Bytes> {
        self.bootstrap.take()
    }

    /// Replace an unsent input datagram with the newest one.
    pub fn set_latest_input(&mut self, bytes: Bytes) {
        self.latest_input = Some(bytes);
    }

    /// Queue a snapshot generation, evicting the oldest unselected generation.
    pub fn push_snapshot(&mut self, bytes: Bytes) {
        if self.snapshots.len() == MAX_PENDING_SNAPSHOTS {
            let _stale = self.snapshots.pop_front();
        }
        self.snapshots.push_back(bytes);
    }

    /// Queue live voice, retaining only three packets per stream.
    pub fn push_voice(&mut self, stream: VoiceStreamId, bytes: Bytes) -> Result<(), QueueError> {
        let is_new = !self.voices.streams.contains_key(&stream);
        if is_new && self.voices.stream_count() >= MAX_AUDIBLE_VOICES {
            return Err(QueueError::VoiceCapacity);
        }
        self.voices.push(stream, bytes);
        Ok(())
    }

    /// Queue one host event without embedding session identities in metrics.
    pub fn push_host_event(&mut self, bytes: Bytes) -> Result<(), QueueError> {
        if self.host_events.len() >= MAX_HOST_EVENTS {
            return Err(QueueError::HostEventCapacity);
        }
        self.host_events.push_back(bytes);
        Ok(())
    }

    /// Pop one host event for synchronous game-host consumption.
    pub fn pop_host_event(&mut self) -> Option<Bytes> {
        self.host_events.pop_front()
    }

    /// Select the next application payload in normative priority order.
    pub fn pop_scheduled(&mut self, minimum_snapshot_due: bool) -> Option<ScheduledPayload> {
        if let Some(bytes) = self.pop_control() {
            return Some(payload(TrafficClass::SessionControl, bytes));
        }
        if let Some(bytes) = self.latest_input.take() {
            return Some(payload(TrafficClass::Input, bytes));
        }
        if minimum_snapshot_due && let Some(bytes) = self.snapshots.pop_front() {
            return Some(payload(TrafficClass::MinimumSnapshot, bytes));
        }
        if let Some(bytes) = self.voices.pop_oldest() {
            return Some(payload(TrafficClass::Voice, bytes));
        }
        self.snapshots
            .pop_front()
            .map(|bytes| payload(TrafficClass::AdditionalSnapshot, bytes))
    }

    fn pop_control(&mut self) -> Option<Bytes> {
        let bytes = self.controls.pop_front()?;
        self.control_bytes = self.control_bytes.saturating_sub(bytes.len());
        Some(bytes)
    }
}

/// Integer token-bucket shaper for per-client and aggregate budgets.
#[derive(Debug, Clone)]
pub struct BandwidthScheduler {
    upstream: TokenBucket,
    downstream: TokenBucket,
}

/// Aggregate egress token bucket shared by every peer in one match.
#[derive(Debug, Clone)]
pub struct MatchEgressBudget {
    egress: TokenBucket,
}

impl BandwidthScheduler {
    /// Create a scheduler at one of the two normative budget tiers.
    #[must_use]
    pub fn new(tier: BudgetTier, now: Duration) -> Self {
        let rates = rates(tier);
        Self {
            upstream: TokenBucket::new(rates.upstream, now),
            downstream: TokenBucket::new(rates.downstream, now),
        }
    }

    /// Reserve estimated application bytes before asking Quinn to send.
    pub fn reserve(
        &mut self,
        direction: TrafficDirection,
        estimated_bytes: usize,
        now: Duration,
    ) -> bool {
        let bits = bytes_to_bits(estimated_bytes);
        match direction {
            TrafficDirection::Upstream => self.upstream.try_take(bits, now),
            TrafficDirection::Downstream => self.downstream.try_take(bits, now),
        }
    }

    /// Atomically reserve both per-client and aggregate match egress.
    pub fn reserve_downstream(
        &mut self,
        match_budget: &mut MatchEgressBudget,
        estimated_bytes: usize,
        now: Duration,
    ) -> bool {
        let bits = bytes_to_bits(estimated_bytes);
        if !self.downstream.can_take(bits, now) || !match_budget.egress.can_take(bits, now) {
            return false;
        }
        self.downstream.take_known(bits);
        match_budget.egress.take_known(bits);
        true
    }

    /// Reconcile the estimate with UDP bytes reported by Quinn.
    pub fn reconcile_udp_bytes(
        &mut self,
        direction: TrafficDirection,
        estimated_bytes: usize,
        actual_udp_bytes: usize,
    ) {
        let estimated = bytes_to_bits(estimated_bytes);
        let actual = bytes_to_bits(actual_udp_bytes);
        match direction {
            TrafficDirection::Upstream => self.upstream.reconcile(estimated, actual),
            TrafficDirection::Downstream => self.downstream.reconcile(estimated, actual),
        }
    }
}

impl MatchEgressBudget {
    /// Create the one aggregate match egress budget for a server match.
    #[must_use]
    pub fn new(tier: BudgetTier, now: Duration) -> Self {
        Self {
            egress: TokenBucket::new(rates(tier).match_egress, now),
        }
    }

    /// Reconcile one peer's estimate against its actual Quinn UDP cost.
    pub fn reconcile_udp_bytes(&mut self, estimated_bytes: usize, actual_udp_bytes: usize) {
        self.egress.reconcile(
            bytes_to_bits(estimated_bytes),
            bytes_to_bits(actual_udp_bytes),
        );
    }
}

#[derive(Debug, Clone, Copy)]
struct Rates {
    upstream: u64,
    downstream: u64,
    match_egress: u64,
}

#[derive(Debug, Clone)]
struct TokenBucket {
    rate_bits_per_second: u64,
    available_bits: u64,
    last_refill: Duration,
}

impl TokenBucket {
    fn new(rate_bits_per_second: u64, now: Duration) -> Self {
        Self {
            rate_bits_per_second,
            available_bits: rate_bits_per_second,
            last_refill: now,
        }
    }

    fn can_take(&mut self, bits: u64, now: Duration) -> bool {
        self.refill(now);
        self.available_bits >= bits
    }

    fn try_take(&mut self, bits: u64, now: Duration) -> bool {
        if !self.can_take(bits, now) {
            return false;
        }
        self.take_known(bits);
        true
    }

    fn take_known(&mut self, bits: u64) {
        self.available_bits = self.available_bits.saturating_sub(bits);
    }

    fn refill(&mut self, now: Duration) {
        let Some(elapsed) = now.checked_sub(self.last_refill) else {
            return;
        };
        let micros = u64::try_from(elapsed.as_micros()).unwrap_or(u64::MAX);
        let added = self
            .rate_bits_per_second
            .saturating_mul(micros)
            .saturating_div(1_000_000);
        self.available_bits = self
            .available_bits
            .saturating_add(added)
            .min(self.rate_bits_per_second);
        self.last_refill = now;
    }

    fn reconcile(&mut self, estimated: u64, actual: u64) {
        if actual > estimated {
            self.take_known(actual - estimated);
        } else {
            self.available_bits = self
                .available_bits
                .saturating_add(estimated - actual)
                .min(self.rate_bits_per_second);
        }
    }
}

const fn rates(tier: BudgetTier) -> Rates {
    match tier {
        BudgetTier::Constrained => Rates {
            upstream: CONSTRAINED_UPSTREAM_BITS_PER_SECOND,
            downstream: CONSTRAINED_DOWNSTREAM_BITS_PER_SECOND,
            match_egress: CONSTRAINED_MATCH_BITS_PER_SECOND,
        },
        BudgetTier::Preferred => Rates {
            upstream: PREFERRED_UPSTREAM_BITS_PER_SECOND,
            downstream: PREFERRED_DOWNSTREAM_BITS_PER_SECOND,
            match_egress: PREFERRED_MATCH_BITS_PER_SECOND,
        },
    }
}

fn payload(class: TrafficClass, bytes: Bytes) -> ScheduledPayload {
    ScheduledPayload { class, bytes }
}

fn bytes_to_bits(bytes: usize) -> u64 {
    u64::try_from(bytes).unwrap_or(u64::MAX).saturating_mul(8)
}
