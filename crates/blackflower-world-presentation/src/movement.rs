use std::num::NonZeroU64;

use glam::{DQuat, DVec3};

const RECONCILIATION_SMOOTHING_SECONDS: f32 = 0.1;

/// Stable source identity used to associate captured movement with its proxy.
#[derive(Debug, Clone, Copy, PartialEq, Eq, PartialOrd, Ord, Hash)]
pub struct MovementSourceId(NonZeroU64);

impl MovementSourceId {
    /// Create a non-zero source identity.
    pub fn new(value: u64) -> Result<Self, PresentationMovementError> {
        NonZeroU64::new(value)
            .map(Self)
            .ok_or(PresentationMovementError::InvalidSourceId)
    }

    /// Return the underlying source identity.
    #[must_use]
    pub const fn get(self) -> u64 {
        self.0.get()
    }
}

/// How one captured movement sample should affect its presentation proxy.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum MovementSampleKind {
    /// Ordinary forward prediction that should remain visually immediate.
    Predicted,
    /// Corrected prediction whose visual discontinuity should be smoothed.
    Reconciled,
    /// A reset prediction timeline that must replace visual state immediately.
    Reset,
}

/// Immutable movement and orientation copied from a prediction source.
#[derive(Debug, Clone, Copy, PartialEq)]
pub struct PresentationMovementSample {
    source: MovementSourceId,
    position_meters: DVec3,
    orientation: DQuat,
    kind: MovementSampleKind,
}

impl PresentationMovementSample {
    /// Validate and copy one prediction-owned movement sample.
    pub fn new(
        source: MovementSourceId,
        position_meters: DVec3,
        orientation: DQuat,
        kind: MovementSampleKind,
    ) -> Result<Self, PresentationMovementError> {
        if !position_meters.is_finite() {
            return Err(PresentationMovementError::NonFinitePosition);
        }
        let orientation = normalize_quaternion(orientation)?;
        Ok(Self {
            source,
            position_meters,
            orientation,
            kind,
        })
    }

    /// Return the captured source identity.
    #[must_use]
    pub const fn source(self) -> MovementSourceId {
        self.source
    }

    /// Return the captured predicted position in metres.
    #[must_use]
    pub const fn position_meters(self) -> DVec3 {
        self.position_meters
    }

    /// Return the captured normalized orientation quaternion.
    #[must_use]
    pub const fn orientation(self) -> DQuat {
        self.orientation
    }

    /// Return how the sample should affect visual reconciliation.
    #[must_use]
    pub const fn kind(self) -> MovementSampleKind {
        self.kind
    }
}

/// Latest successfully committed presentation-owned local movement proxy.
#[derive(Debug, Clone, Copy, PartialEq)]
pub struct MovementProxy {
    source: MovementSourceId,
    predicted_position_meters: DVec3,
    predicted_orientation: DQuat,
    visual_position_meters: DVec3,
    visual_orientation: DQuat,
    correction_active: bool,
}

impl MovementProxy {
    /// Return the prediction source represented by this proxy.
    #[must_use]
    pub const fn source(self) -> MovementSourceId {
        self.source
    }

    /// Return the latest captured predicted position in metres.
    #[must_use]
    pub const fn predicted_position_meters(self) -> DVec3 {
        self.predicted_position_meters
    }

    /// Return the latest captured predicted orientation.
    #[must_use]
    pub const fn predicted_orientation(self) -> DQuat {
        self.predicted_orientation
    }

    /// Return the presentation-owned position after visual reconciliation.
    #[must_use]
    pub const fn visual_position_meters(self) -> DVec3 {
        self.visual_position_meters
    }

    /// Return the presentation-owned orientation after visual reconciliation.
    #[must_use]
    pub const fn visual_orientation(self) -> DQuat {
        self.visual_orientation
    }

    /// Return whether a visual correction remains in progress.
    #[must_use]
    pub const fn correction_active(self) -> bool {
        self.correction_active
    }
}

/// Failure while validating or accessing presentation movement state.
#[derive(Debug, Clone, Copy, PartialEq, Eq, thiserror::Error)]
pub enum PresentationMovementError {
    /// Source identities must be non-zero.
    #[error("presentation movement source identity must be non-zero")]
    InvalidSourceId,
    /// Positions crossing the presentation boundary must be finite.
    #[error("presentation movement position must contain only finite values")]
    NonFinitePosition,
    /// Orientations must be finite, non-zero quaternions.
    #[error("presentation movement orientation must be a finite non-zero quaternion")]
    InvalidOrientation,
    /// Presentation-owned movement storage was poisoned by a previous panic.
    #[error("presentation movement state is unavailable")]
    StateUnavailable,
}

#[derive(Debug, Clone, Copy)]
struct MovementProxyState {
    source: MovementSourceId,
    predicted_position_meters: DVec3,
    predicted_orientation: DQuat,
    visual_position_meters: DVec3,
    visual_orientation: DQuat,
    correction_remaining_seconds: f32,
}

impl MovementProxyState {
    fn from_sample(sample: PresentationMovementSample) -> Self {
        Self {
            source: sample.source,
            predicted_position_meters: sample.position_meters,
            predicted_orientation: sample.orientation,
            visual_position_meters: sample.position_meters,
            visual_orientation: sample.orientation,
            correction_remaining_seconds: 0.0,
        }
    }

    fn snapshot(self) -> MovementProxy {
        MovementProxy {
            source: self.source,
            predicted_position_meters: self.predicted_position_meters,
            predicted_orientation: self.predicted_orientation,
            visual_position_meters: self.visual_position_meters,
            visual_orientation: self.visual_orientation,
            correction_active: self.correction_remaining_seconds > 0.0,
        }
    }
}

#[derive(Debug, Default)]
pub(crate) struct PresentationMovementState {
    pending: Option<PresentationMovementSample>,
    captured: Option<PresentationMovementSample>,
    working: Option<MovementProxyState>,
    committed: Option<MovementProxyState>,
}

impl PresentationMovementState {
    pub(crate) fn set_pending(&mut self, sample: Option<PresentationMovementSample>) {
        self.pending = sample;
    }

    pub(crate) fn begin_frame(&mut self) {
        self.captured = None;
        self.working = self.committed;
    }

    pub(crate) fn capture(&mut self) {
        self.captured = self.pending;
    }

    pub(crate) fn create_missing_proxy(&mut self) {
        let Some(sample) = self.captured else {
            return;
        };
        if self
            .working
            .is_none_or(|proxy| proxy.source != sample.source)
        {
            self.working = Some(MovementProxyState::from_sample(sample));
        }
    }

    pub(crate) fn retire_stale_proxy(&mut self) {
        if self.captured.is_none() {
            self.working = None;
        }
    }

    pub(crate) fn sample_prediction(&mut self) {
        let (Some(sample), Some(proxy)) = (self.captured, self.working.as_mut()) else {
            return;
        };
        if proxy.source != sample.source {
            return;
        }

        proxy.predicted_position_meters = sample.position_meters;
        proxy.predicted_orientation = sample.orientation;
        match sample.kind {
            MovementSampleKind::Predicted if proxy.correction_remaining_seconds <= 0.0 => {
                proxy.visual_position_meters = sample.position_meters;
                proxy.visual_orientation = sample.orientation;
            }
            MovementSampleKind::Reconciled => {
                if transforms_differ(proxy, sample) {
                    proxy.correction_remaining_seconds = RECONCILIATION_SMOOTHING_SECONDS;
                }
            }
            MovementSampleKind::Reset => {
                proxy.visual_position_meters = sample.position_meters;
                proxy.visual_orientation = sample.orientation;
                proxy.correction_remaining_seconds = 0.0;
            }
            MovementSampleKind::Predicted => {}
        }
    }

    pub(crate) fn smooth_correction(&mut self, delta_seconds: f32) {
        let Some(proxy) = self.working.as_mut() else {
            return;
        };
        let remaining = proxy.correction_remaining_seconds;
        if remaining <= 0.0 {
            return;
        }
        let elapsed = delta_seconds.min(remaining);
        let amount = f64::from(elapsed / remaining);
        proxy.visual_position_meters = proxy
            .visual_position_meters
            .lerp(proxy.predicted_position_meters, amount);
        proxy.visual_orientation = proxy
            .visual_orientation
            .slerp(proxy.predicted_orientation, amount);
        proxy.correction_remaining_seconds = remaining - elapsed;
        if proxy.correction_remaining_seconds <= f32::EPSILON {
            proxy.visual_position_meters = proxy.predicted_position_meters;
            proxy.visual_orientation = proxy.predicted_orientation;
            proxy.correction_remaining_seconds = 0.0;
        }
    }

    pub(crate) fn release_captured(&mut self) {
        self.captured = None;
    }

    pub(crate) fn commit_frame(&mut self) {
        self.committed = self.working;
        self.captured = None;
    }

    pub(crate) fn discard_frame(&mut self) {
        self.working = self.committed;
        self.captured = None;
    }

    pub(crate) fn committed(&self) -> Option<MovementProxy> {
        self.committed.map(MovementProxyState::snapshot)
    }

    pub(crate) fn working(&self) -> Option<MovementProxy> {
        self.working.map(MovementProxyState::snapshot)
    }
}

fn normalize_quaternion(orientation: DQuat) -> Result<DQuat, PresentationMovementError> {
    let norm_squared = orientation.length_squared();
    if !norm_squared.is_finite() || norm_squared <= f64::EPSILON {
        return Err(PresentationMovementError::InvalidOrientation);
    }
    Ok(orientation.normalize())
}

fn transforms_differ(proxy: &MovementProxyState, sample: PresentationMovementSample) -> bool {
    if !proxy
        .visual_position_meters
        .abs_diff_eq(sample.position_meters, f64::EPSILON)
    {
        return true;
    }
    let orientation_dot = proxy.visual_orientation.dot(sample.orientation).abs();
    orientation_dot < 1.0 - f64::EPSILON
}
