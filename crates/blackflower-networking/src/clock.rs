use std::collections::VecDeque;
use std::time::Duration;

use crate::SimulationTick;

/// Authoritative simulation rate used by network timing contracts.
pub const NETWORK_TICK_RATE_HZ: u64 = 240;
/// Admission and path-change sample count.
pub const INITIAL_TIME_SYNC_SAMPLES: u8 = 8;
/// Interval between initial and path-change samples.
pub const INITIAL_TIME_SYNC_INTERVAL: Duration = Duration::from_millis(100);
/// Interval between active-state samples.
pub const ACTIVE_TIME_SYNC_INTERVAL: Duration = Duration::from_secs(1);
/// Maximum interval without a valid clock sample before temporal commands stop.
pub const CLOCK_SAMPLE_TIMEOUT: Duration = Duration::from_secs(3);
/// Initial conservative input lead.
pub const INITIAL_INPUT_LEAD_TICKS: u64 = 12;

/// One completed four-timestamp clock exchange, in monotonic microseconds.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub struct ClockSample {
    /// Client request transmit time.
    pub client_send_micros: u64,
    /// Server request receive time.
    pub server_receive_micros: u64,
    /// Server response transmit time.
    pub server_send_micros: u64,
    /// Client response receive time.
    pub client_receive_micros: u64,
}

/// Safety classification derived from uncertainty and sample freshness.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum ClockSafety {
    /// Activation is allowed because uncertainty is at most two ticks.
    ActivationReady,
    /// Clock remains usable but is not safe for first activation.
    Tracking,
    /// Uncertainty exceeded four ticks for three consecutive samples.
    Degraded,
    /// Temporal commands must be blocked.
    Blocked,
}

/// Invalid monotonic timestamps in a time-sync exchange.
#[derive(Debug, Clone, Copy, PartialEq, Eq, thiserror::Error)]
pub enum ClockError {
    /// A timestamp moved backwards within one participant's time domain.
    #[error("time-sync timestamps are not monotonic")]
    NonMonotonic,
    /// The server reported more processing time than total round-trip time.
    #[error("time-sync sample has a negative network delay")]
    NegativeDelay,
    /// Clock arithmetic exceeded the supported integer domain.
    #[error("time-sync arithmetic overflow")]
    Arithmetic,
}

/// Deterministic minimum-delay clock filter with monotonic slew mapping.
#[derive(Debug, Clone)]
pub struct ClockFilter {
    samples: VecDeque<FilteredSample>,
    offset_micros: i128,
    uncertainty_micros: u64,
    consecutive_degraded: u8,
    last_sample_at: Option<Duration>,
    last_mapped_micros: u64,
}

impl Default for ClockFilter {
    fn default() -> Self {
        Self::new()
    }
}

impl ClockFilter {
    /// Create an empty clock filter using the conservative initial lead.
    #[must_use]
    pub fn new() -> Self {
        Self {
            samples: VecDeque::with_capacity(usize::from(INITIAL_TIME_SYNC_SAMPLES)),
            offset_micros: 0,
            uncertainty_micros: ticks_to_micros(INITIAL_INPUT_LEAD_TICKS),
            consecutive_degraded: 0,
            last_sample_at: None,
            last_mapped_micros: 0,
        }
    }

    /// Add one validated exchange and update the minimum-delay estimate.
    pub fn observe(&mut self, sample: ClockSample, now: Duration) -> Result<(), ClockError> {
        let filtered = FilteredSample::try_from(sample)?;
        if self.samples.len() == usize::from(INITIAL_TIME_SYNC_SAMPLES) {
            let _oldest = self.samples.pop_front();
        }
        self.samples.push_back(filtered);
        let best = self
            .samples
            .iter()
            .min_by_key(|candidate| candidate.delay_micros)
            .copied()
            .ok_or(ClockError::Arithmetic)?;
        self.offset_micros = best.offset_micros;
        self.uncertainty_micros = best.delay_micros.div_ceil(2);
        self.last_sample_at = Some(now);
        if self.uncertainty_ticks() > 4 {
            self.consecutive_degraded = self.consecutive_degraded.saturating_add(1);
        } else {
            self.consecutive_degraded = 0;
        }
        Ok(())
    }

    /// Map local monotonic time into the estimated server domain without reversal.
    pub fn map_local_micros(&mut self, local_micros: u64) -> Result<u64, ClockError> {
        let mapped = i128::from(local_micros)
            .checked_add(self.offset_micros)
            .ok_or(ClockError::Arithmetic)?;
        let non_negative = u64::try_from(mapped.max(0)).map_err(|_error| ClockError::Arithmetic)?;
        let monotonic = non_negative.max(self.last_mapped_micros);
        self.last_mapped_micros = monotonic;
        Ok(monotonic)
    }

    /// Return uncertainty rounded upward to authoritative ticks.
    #[must_use]
    pub fn uncertainty_ticks(&self) -> u64 {
        micros_to_ticks_ceil(self.uncertainty_micros)
    }

    /// Classify clock safety at the supplied monotonic instant.
    #[must_use]
    pub fn safety(&self, now: Duration) -> ClockSafety {
        if self.last_sample_at.is_none_or(|then| {
            now.checked_sub(then)
                .is_none_or(|elapsed| elapsed >= CLOCK_SAMPLE_TIMEOUT)
        }) || self.uncertainty_ticks() > 8
        {
            ClockSafety::Blocked
        } else if self.consecutive_degraded >= 3 {
            ClockSafety::Degraded
        } else if self.uncertainty_ticks() <= 2 {
            ClockSafety::ActivationReady
        } else {
            ClockSafety::Tracking
        }
    }
}

/// Deterministic cadence for initial, active, and path-change exchanges.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub struct TimeSyncSchedule {
    remaining_burst: u8,
    next_at: Duration,
    active: bool,
}

impl TimeSyncSchedule {
    /// Begin the eight-sample admission burst immediately.
    #[must_use]
    pub const fn admission(now: Duration) -> Self {
        Self {
            remaining_burst: INITIAL_TIME_SYNC_SAMPLES,
            next_at: now,
            active: false,
        }
    }

    /// Restart the eight-sample burst after a validated path change.
    pub fn path_changed(&mut self, now: Duration) {
        self.remaining_burst = INITIAL_TIME_SYNC_SAMPLES;
        self.next_at = now;
    }

    /// Switch to the one-second active cadence after synchronization.
    pub fn set_active(&mut self, now: Duration) {
        self.active = true;
        if self.remaining_burst == 0 {
            self.next_at = now.saturating_add(ACTIVE_TIME_SYNC_INTERVAL);
        }
    }

    /// Consume one due exchange and schedule the next one.
    pub fn take_due(&mut self, now: Duration) -> bool {
        if now < self.next_at {
            return false;
        }
        if self.remaining_burst > 0 {
            self.remaining_burst -= 1;
            let interval = if self.remaining_burst == 0 && self.active {
                ACTIVE_TIME_SYNC_INTERVAL
            } else {
                INITIAL_TIME_SYNC_INTERVAL
            };
            self.next_at = now.saturating_add(interval);
        } else if self.active {
            self.next_at = now.saturating_add(ACTIVE_TIME_SYNC_INTERVAL);
        } else {
            return false;
        }
        true
    }
}

/// Compute the four-tick-aligned input lead from smoothed RTT statistics.
#[must_use]
pub fn input_lead_ticks(smoothed_rtt: Duration, rtt_variance: Duration) -> u64 {
    let half_rtt = smoothed_rtt / 2;
    let allowance = half_rtt.saturating_add(rtt_variance.saturating_mul(2));
    let ticks = micros_to_ticks_ceil(duration_micros_saturating(allowance));
    align_up(ticks, 4).clamp(4, 24)
}

/// Convert an estimated server time into an authoritative tick.
#[must_use]
pub fn server_micros_to_tick(server_micros: u64) -> SimulationTick {
    SimulationTick::new(server_micros.saturating_mul(NETWORK_TICK_RATE_HZ) / 1_000_000)
}

#[derive(Debug, Clone, Copy)]
struct FilteredSample {
    delay_micros: u64,
    offset_micros: i128,
}

impl TryFrom<ClockSample> for FilteredSample {
    type Error = ClockError;

    fn try_from(sample: ClockSample) -> Result<Self, Self::Error> {
        if sample.client_receive_micros < sample.client_send_micros
            || sample.server_send_micros < sample.server_receive_micros
        {
            return Err(ClockError::NonMonotonic);
        }
        let round_trip = sample.client_receive_micros - sample.client_send_micros;
        let server_work = sample.server_send_micros - sample.server_receive_micros;
        let delay_micros = round_trip
            .checked_sub(server_work)
            .ok_or(ClockError::NegativeDelay)?;
        let outbound =
            i128::from(sample.server_receive_micros) - i128::from(sample.client_send_micros);
        let inbound =
            i128::from(sample.server_send_micros) - i128::from(sample.client_receive_micros);
        Ok(Self {
            delay_micros,
            offset_micros: (outbound + inbound) / 2,
        })
    }
}

const fn ticks_to_micros(ticks: u64) -> u64 {
    ticks
        .saturating_mul(1_000_000)
        .div_ceil(NETWORK_TICK_RATE_HZ)
}

const fn micros_to_ticks_ceil(micros: u64) -> u64 {
    micros
        .saturating_mul(NETWORK_TICK_RATE_HZ)
        .div_ceil(1_000_000)
}

fn duration_micros_saturating(value: Duration) -> u64 {
    u64::try_from(value.as_micros()).unwrap_or(u64::MAX)
}

const fn align_up(value: u64, quantum: u64) -> u64 {
    value.saturating_add(quantum - 1) / quantum * quantum
}
