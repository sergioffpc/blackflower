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

    pub const fn next(self) -> Self {
        Self(self.0 + 1)
    }
}

impl std::ops::Rem<u64> for Tick {
    type Output = u64;

    fn rem(self, rhs: u64) -> Self::Output {
        self.0 % rhs
    }
}

impl From<u64> for Tick {
    fn from(value: u64) -> Self {
        Self(value)
    }
}

impl From<Tick> for u64 {
    fn from(value: Tick) -> Self {
        value.0
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

    #[allow(clippy::infinite_loop, reason = "tick scheduler runs until SIGTERM")]
    pub fn start<F>(&self, mut do_tick: F) -> anyhow::Result<()>
    where
        F: FnMut(Tick, Duration),
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

            do_tick(current_tick, self.tick_duration);

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
