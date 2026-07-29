use std::collections::BTreeMap;
use std::num::NonZeroUsize;

use crate::{DeltaError, Snapshot, SnapshotDelta, SnapshotTick};

/// Bounded per-client history of sent snapshots and the latest acknowledged baseline.
#[derive(Debug, Clone)]
pub struct BaselineTracker<S> {
    maximum_pending: NonZeroUsize,
    acknowledged: Option<Snapshot<S>>,
    pending: BTreeMap<SnapshotTick, Snapshot<S>>,
}

impl<S> BaselineTracker<S> {
    /// Construct a tracker with an explicit bound on unacknowledged snapshots.
    #[must_use]
    pub const fn new(maximum_pending: NonZeroUsize) -> Self {
        Self {
            maximum_pending,
            acknowledged: None,
            pending: BTreeMap::new(),
        }
    }

    /// Return the latest acknowledged snapshot used as the next delta baseline.
    #[must_use]
    pub const fn baseline(&self) -> Option<&Snapshot<S>> {
        self.acknowledged.as_ref()
    }

    /// Return the latest acknowledged tick.
    #[must_use]
    pub fn acknowledged_tick(&self) -> Option<SnapshotTick> {
        self.acknowledged.as_ref().map(Snapshot::tick)
    }

    /// Return the number of sent snapshots awaiting acknowledgement.
    #[must_use]
    pub fn pending_len(&self) -> usize {
        self.pending.len()
    }

    /// Retain a sent snapshot for a future acknowledgement.
    ///
    /// The oldest pending snapshot is evicted when the configured bound is
    /// exceeded. A later acknowledgement for an evicted tick is rejected so the
    /// caller can request or send a full snapshot.
    pub fn record_sent(
        &mut self,
        snapshot: Snapshot<S>,
    ) -> Result<Option<SnapshotTick>, BaselineError> {
        let tick = snapshot.tick();
        if let Some(latest) = self.latest_tick()
            && tick <= latest
        {
            return Err(BaselineError::NonIncreasingSnapshot { tick, latest });
        }
        self.pending.insert(tick, snapshot);
        Ok(self.evict_oldest_pending())
    }

    /// Promote one sent snapshot to the acknowledged delta baseline.
    ///
    /// Acknowledgements are cumulative: pending snapshots at or before `tick`
    /// are discarded after the exact acknowledged snapshot is promoted.
    pub fn acknowledge(&mut self, tick: SnapshotTick) -> Result<(), BaselineError> {
        if let Some(acknowledged) = self.acknowledged_tick() {
            if tick == acknowledged {
                return Ok(());
            }
            if tick < acknowledged {
                return Err(BaselineError::AcknowledgementRegressed { tick, acknowledged });
            }
        }
        let snapshot = self
            .pending
            .remove(&tick)
            .ok_or(BaselineError::UnknownAcknowledgement { tick })?;
        self.pending
            .retain(|pending_tick, _snapshot| *pending_tick > tick);
        self.acknowledged = Some(snapshot);
        Ok(())
    }

    fn latest_tick(&self) -> Option<SnapshotTick> {
        self.pending
            .last_key_value()
            .map(|(&tick, _snapshot)| tick)
            .or_else(|| self.acknowledged_tick())
    }

    fn evict_oldest_pending(&mut self) -> Option<SnapshotTick> {
        if self.pending.len() <= self.maximum_pending.get() {
            return None;
        }
        let oldest = self
            .pending
            .first_key_value()
            .map(|(&tick, _snapshot)| tick)?;
        self.pending.remove(&oldest);
        Some(oldest)
    }
}

impl<S: Clone + Eq> BaselineTracker<S> {
    /// Build a delta against the latest acknowledged snapshot.
    pub fn build_delta(&self, current: &Snapshot<S>) -> Result<SnapshotDelta<S>, BaselineError> {
        SnapshotDelta::between(current, self.baseline()).map_err(BaselineError::Delta)
    }
}

/// Invalid sent-snapshot or acknowledgement progression.
#[derive(Debug, Clone, Copy, PartialEq, Eq, thiserror::Error)]
pub enum BaselineError {
    /// Sent snapshots were not recorded in increasing tick order.
    #[error("sent snapshot tick {tick} must be newer than {latest}")]
    NonIncreasingSnapshot {
        /// Rejected snapshot tick.
        tick: SnapshotTick,
        /// Latest retained or acknowledged tick.
        latest: SnapshotTick,
    },
    /// An acknowledgement moved behind the current baseline.
    #[error("acknowledgement tick {tick} is older than baseline tick {acknowledged}")]
    AcknowledgementRegressed {
        /// Rejected acknowledgement tick.
        tick: SnapshotTick,
        /// Current acknowledged baseline tick.
        acknowledged: SnapshotTick,
    },
    /// No pending sent snapshot matched the acknowledgement.
    #[error("snapshot acknowledgement references unknown tick {tick}")]
    UnknownAcknowledgement {
        /// Unknown acknowledgement tick.
        tick: SnapshotTick,
    },
    /// Delta construction rejected the selected baseline.
    #[error(transparent)]
    Delta(#[from] DeltaError),
}
