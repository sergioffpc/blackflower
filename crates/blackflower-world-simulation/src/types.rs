use std::fmt;

/// Monotonic fixed-step tick in the authoritative simulation timeline.
#[derive(Debug, Default, Clone, Copy, PartialEq, Eq, PartialOrd, Ord, Hash)]
pub struct SimulationTick(u64);

impl SimulationTick {
    /// Initial sealed state before the first authoritative simulation tick.
    pub const ZERO: Self = Self(0);

    /// Construct an authoritative tick from its protocol value.
    #[must_use]
    pub const fn new(value: u64) -> Self {
        Self(value)
    }

    /// Return the protocol value.
    #[must_use]
    pub const fn get(self) -> u64 {
        self.0
    }

    pub(crate) const fn checked_next(self) -> Option<Self> {
        match self.0.checked_add(1) {
            Some(value) => Some(Self(value)),
            None => None,
        }
    }
}

impl fmt::Display for SimulationTick {
    fn fmt(&self, formatter: &mut fmt::Formatter<'_>) -> fmt::Result {
        self.0.fmt(formatter)
    }
}

#[derive(Debug, Clone, Copy, PartialEq, Eq, thiserror::Error)]
#[error("simulation tick overflow")]
pub(crate) struct SimulationTickOverflow;

#[cfg(test)]
#[path = "../tests/unit/types.rs"]
mod tests;
