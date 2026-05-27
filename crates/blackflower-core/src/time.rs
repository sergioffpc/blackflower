//! Time and tick utilities.
//!
//! The engine uses a fixed-step tick at 60 Hz. The `Tick` type identifies
//! a specific simulation step; `TICK_DURATION` is the wall-clock interval
//! between ticks; `TICK_DT_SECS` is the floating-point delta passed to
//! systems that integrate over time.

use std::time::Duration;

use serde::{Deserialize, Serialize};

/// Simulation tick rate, in Hertz.
pub const TICK_HZ: u32 = 60;

/// Duration of a single simulation tick.
///
/// `1 / 60 s = 16_666.666... µs`. We round to the nearest microsecond.
pub const TICK_DURATION: Duration = Duration::from_micros(16_667);

/// Delta time passed to simulation systems, in seconds.
///
/// Equals `1.0 / TICK_HZ as f32`. Stored as a constant rather than computed
/// to avoid floating-point drift and to allow use in `const` contexts.
#[allow(clippy::as_conversions)]
pub const TICK_DT_SECS: f32 = 1.0 / TICK_HZ as f32;

/// Identifier of a single simulation step.
#[derive(
    Clone, Copy, Debug, Default, PartialEq, Eq, PartialOrd, Ord, Hash, Serialize, Deserialize,
)]
#[repr(transparent)]
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
        write!(f, "tick {}", self.0)
    }
}
