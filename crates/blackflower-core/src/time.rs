//! Time and tick utilities.
//!
//! The engine uses a fixed-step tick at 60 Hz. The `Tick` type identifies
//! a specific simulation step; `TICK_DURATION` is the wall-clock interval
//! between ticks; `TICK_DT_SECS` is the floating-point delta passed to
//! systems that integrate over time.

use std::time::{Duration, Instant};

use serde::{Deserialize, Serialize};
use tracing::{info, warn};

/// Identifier of a single simulation step.
#[repr(transparent)]
#[derive(
    Clone, Copy, Debug, Default, PartialEq, Eq, PartialOrd, Ord, Hash, Serialize, Deserialize,
)]
pub struct Tick(u64);

impl Tick {
    pub const ZERO: Self = Self(0);

    #[must_use]
    pub const fn next(self) -> Self {
        Self(self.0 + 1)
    }
}

impl std::fmt::Display for Tick {
    fn fmt(&self, f: &mut std::fmt::Formatter<'_>) -> std::fmt::Result {
        write!(f, "{}", self.0)
    }
}

pub struct TickScheduler {
    tick_hz: u64,
    tick_duration: Duration,
}

impl TickScheduler {
    pub fn new(tick_hz: u64) -> Self {
        Self {
            tick_hz,
            tick_duration: Duration::from_secs_f64(1.0 / tick_hz as f64),
        }
    }

    pub fn start<F>(&self, mut do_tick: F) -> anyhow::Result<()>
    where
        F: FnMut(Tick),
    {
        info!(
            tick_hz = self.tick_hz,
            tick_duration_ms = self.tick_duration.as_millis(),
            "tick scheduler"
        );

        let mut current_tick = Tick::ZERO;
        let mut next_tick_instant = Instant::now() + self.tick_duration;

        loop {
            let current_tick_instant = Instant::now();

            do_tick(current_tick);

            let now = Instant::now();
            if now < next_tick_instant {
                std::thread::sleep(next_tick_instant - now);
            } else {
                let overrun = now - current_tick_instant;
                warn!(
                    tick = %current_tick,
                    overrun_us = u64::try_from(overrun.as_micros())?,
                    "tick scheduler overran"
                );
            }

            current_tick = current_tick.next();
            next_tick_instant += self.tick_duration;
        }
    }
}
