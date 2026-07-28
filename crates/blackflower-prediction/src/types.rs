use std::fmt;

/// Monotonic fixed-step tick in the local prediction timeline.
#[derive(Debug, Default, Clone, Copy, PartialEq, Eq, PartialOrd, Ord, Hash)]
pub struct PredictionTick(u64);

impl PredictionTick {
    /// Initial sealed prediction state before the first simulated tick.
    pub const ZERO: Self = Self(0);

    /// Construct a prediction tick from its protocol value.
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

impl fmt::Display for PredictionTick {
    fn fmt(&self, formatter: &mut fmt::Formatter<'_>) -> fmt::Result {
        self.0.fmt(formatter)
    }
}

/// Monotonic sequence assigned to a locally produced control frame.
#[derive(Debug, Default, Clone, Copy, PartialEq, Eq, PartialOrd, Ord, Hash)]
pub struct InputSequence(u64);

impl InputSequence {
    /// Construct an input sequence from its protocol value.
    #[must_use]
    pub const fn new(value: u64) -> Self {
        Self(value)
    }

    /// Return the protocol value.
    #[must_use]
    pub const fn get(self) -> u64 {
        self.0
    }
}

impl fmt::Display for InputSequence {
    fn fmt(&self, formatter: &mut fmt::Formatter<'_>) -> fmt::Result {
        self.0.fmt(formatter)
    }
}

/// Which pass through the prediction pipeline is being executed.
#[derive(Debug, Default, Clone, Copy, PartialEq, Eq, Hash)]
pub enum PredictionPass {
    /// First execution of a tick from current device input.
    #[default]
    Forward,
    /// Reexecution of a historical tick during authoritative reconciliation.
    Resimulation,
}
