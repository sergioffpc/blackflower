use std::f64::consts::{PI, TAU};

/// Semantic result of comparing predicted state with an authoritative state.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum PredictionStateComparison {
    /// Every field satisfies its gameplay-owned comparison policy.
    WithinTolerance,
    /// At least one field requires authoritative correction.
    CorrectionRequired,
}

impl PredictionStateComparison {
    /// Convert a gameplay-owned comparison result into an explicit decision.
    #[must_use]
    pub const fn from_within_tolerance(within_tolerance: bool) -> Self {
        if within_tolerance {
            Self::WithinTolerance
        } else {
            Self::CorrectionRequired
        }
    }

    /// Return whether prediction may remain on its current timeline.
    #[must_use]
    pub const fn is_within_tolerance(self) -> bool {
        matches!(self, Self::WithinTolerance)
    }
}

/// Invalid tolerance supplied by gameplay policy.
#[derive(Debug, Clone, Copy, PartialEq, Eq, thiserror::Error)]
pub enum ToleranceError {
    /// An absolute tolerance was negative or non-finite.
    #[error("absolute prediction tolerance must be finite and non-negative")]
    InvalidAbsoluteTolerance,
    /// An angular tolerance was outside the shortest-arc range.
    #[error("angular prediction tolerance must be finite and between zero and pi")]
    InvalidAngularTolerance,
}

/// Maximum absolute error accepted for one continuous gameplay field.
///
/// This is a comparison primitive, not a global engine epsilon. The gameplay
/// codec constructs a separate value for each domain, such as position,
/// velocity, or stamina, and still compares discrete fields exactly.
#[derive(Debug, Clone, Copy, PartialEq)]
pub struct AbsoluteTolerance {
    maximum_error: f64,
}

impl AbsoluteTolerance {
    /// Validate a maximum absolute error in the field's own units.
    pub fn new(maximum_error: f64) -> Result<Self, ToleranceError> {
        if !maximum_error.is_finite() || maximum_error < 0.0 {
            return Err(ToleranceError::InvalidAbsoluteTolerance);
        }
        Ok(Self { maximum_error })
    }

    /// Return the accepted absolute error in the field's own units.
    #[must_use]
    pub const fn maximum_error(self) -> f64 {
        self.maximum_error
    }

    /// Compare two finite scalar values using this field-specific tolerance.
    #[must_use]
    pub fn compare(self, predicted: f64, authoritative: f64) -> PredictionStateComparison {
        PredictionStateComparison::from_within_tolerance(
            predicted.is_finite()
                && authoritative.is_finite()
                && (predicted - authoritative).abs() <= self.maximum_error,
        )
    }
}

/// Maximum shortest-arc error accepted for one angular field.
#[derive(Debug, Clone, Copy, PartialEq)]
pub struct AngularTolerance {
    maximum_error_radians: f64,
}

impl AngularTolerance {
    /// Validate a maximum angular error in radians.
    pub fn new(maximum_error_radians: f64) -> Result<Self, ToleranceError> {
        if !(0.0..=PI).contains(&maximum_error_radians) {
            return Err(ToleranceError::InvalidAngularTolerance);
        }
        Ok(Self {
            maximum_error_radians,
        })
    }

    /// Return the accepted shortest-arc error in radians.
    #[must_use]
    pub const fn maximum_error_radians(self) -> f64 {
        self.maximum_error_radians
    }

    /// Compare two finite angles using their shortest distance around a turn.
    #[must_use]
    pub fn compare(self, predicted: f64, authoritative: f64) -> PredictionStateComparison {
        if !predicted.is_finite() || !authoritative.is_finite() {
            return PredictionStateComparison::CorrectionRequired;
        }
        let forward_distance = (predicted - authoritative).rem_euclid(TAU);
        let shortest_distance = forward_distance.min(TAU - forward_distance);
        PredictionStateComparison::from_within_tolerance(
            shortest_distance <= self.maximum_error_radians,
        )
    }
}
