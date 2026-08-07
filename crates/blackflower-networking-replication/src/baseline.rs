use std::collections::BTreeMap;
use std::sync::Arc;

use blackflower_networking::{ProjectionDigest, ProtocolRevision, SnapshotAppliedAck};

use crate::{DeltaError, Snapshot, SnapshotDelta, SnapshotError, SnapshotTick};

/// Maximum sent snapshot generations retained for an applied ACK.
pub const MAX_SENT_SNAPSHOTS: usize = 32;

/// Bounded per-client history and latest exactly acknowledged baseline.
#[derive(Debug, Clone)]
pub struct BaselineTracker {
    revision: ProtocolRevision,
    acknowledged: Option<SentSnapshot>,
    pending: BTreeMap<SnapshotTick, PendingSnapshot>,
}

impl BaselineTracker {
    /// Construct the fixed 32-snapshot tracker for one protocol revision.
    #[must_use]
    pub const fn new(revision: ProtocolRevision) -> Self {
        Self {
            revision,
            acknowledged: None,
            pending: BTreeMap::new(),
        }
    }

    /// Return the exact applied snapshot used as the next delta baseline.
    #[must_use]
    pub fn baseline(&self) -> Option<&Snapshot> {
        self.acknowledged
            .as_ref()
            .map(|sent| sent.snapshot.as_ref())
    }

    /// Return the latest acknowledged tick.
    #[must_use]
    pub fn acknowledged_tick(&self) -> Option<SnapshotTick> {
        self.acknowledged.as_ref().map(|sent| sent.snapshot.tick())
    }

    /// Return the exact digest promoted with the current baseline.
    #[must_use]
    pub fn acknowledged_digest(&self) -> Option<ProjectionDigest> {
        self.acknowledged.as_ref().map(|sent| sent.digest)
    }

    /// Return the number of sent snapshots awaiting an applied ACK.
    #[must_use]
    pub fn pending_len(&self) -> usize {
        self.pending.len()
    }

    /// Retain a sent snapshot and its locally computed canonical digest.
    pub fn record_sent(
        &mut self,
        snapshot: Snapshot,
    ) -> Result<Option<SnapshotTick>, BaselineError> {
        let delta = SnapshotDelta::between(&snapshot, self.baseline())?;
        self.record_sent_delta(snapshot, delta)
    }

    /// Retain a sent snapshot using the exact delta already built for transport.
    pub fn record_sent_delta(
        &mut self,
        snapshot: Snapshot,
        delta: SnapshotDelta,
    ) -> Result<Option<SnapshotTick>, BaselineError> {
        let tick = snapshot.tick();
        if let Some(latest) = self.latest_tick()
            && tick <= latest
        {
            return Err(BaselineError::NonIncreasingSnapshot { tick, latest });
        }
        if delta.tick() != tick || delta.baseline() != self.acknowledged_tick() {
            return Err(BaselineError::DeltaDoesNotMatchBaseline {
                snapshot: tick,
                delta: delta.tick(),
                expected_baseline: self.acknowledged_tick(),
                actual_baseline: delta.baseline(),
            });
        }
        let digest = snapshot.digest(self.revision)?;
        if let Some(previous_tick) = self
            .pending
            .last_key_value()
            .map(|(&previous_tick, _pending)| previous_tick)
            && let Some(previous) = self.pending.get_mut(&previous_tick)
        {
            previous.snapshot = None;
        }
        self.pending.insert(
            tick,
            PendingSnapshot {
                baseline: self
                    .acknowledged
                    .as_ref()
                    .map(|sent| Arc::clone(&sent.snapshot)),
                delta,
                snapshot: Some(snapshot),
                digest,
            },
        );
        Ok(self.evict_oldest_pending())
    }

    /// Promote only an exact tick-and-digest application ACK.
    pub fn acknowledge(&mut self, ack: SnapshotAppliedAck) -> Result<(), BaselineError> {
        let tick = SnapshotTick::new(ack.snapshot_tick.get());
        if let Some(current) = &self.acknowledged {
            if tick == current.snapshot.tick() {
                return if ack.projection_digest == current.digest {
                    Ok(())
                } else {
                    Err(BaselineError::DigestMismatch { tick })
                };
            }
            if tick < current.snapshot.tick() {
                return Err(BaselineError::AcknowledgementRegressed {
                    tick,
                    acknowledged: current.snapshot.tick(),
                });
            }
        }
        let sent = self
            .pending
            .get(&tick)
            .ok_or(BaselineError::UnknownAcknowledgement { tick })?;
        if sent.digest != ack.projection_digest {
            return Err(BaselineError::DigestMismatch { tick });
        }
        let promoted = self
            .pending
            .remove(&tick)
            .ok_or(BaselineError::UnknownAcknowledgement { tick })?;
        let snapshot = promoted
            .snapshot
            .map_or_else(|| promoted.delta.apply(promoted.baseline.as_deref()), Ok)?;
        self.pending
            .retain(|pending_tick, _snapshot| *pending_tick > tick);
        self.acknowledged = Some(SentSnapshot {
            snapshot: Arc::new(snapshot),
            digest: promoted.digest,
        });
        Ok(())
    }

    /// Build component operations against only the exact applied baseline.
    pub fn build_delta(&self, current: &Snapshot) -> Result<SnapshotDelta, BaselineError> {
        SnapshotDelta::between(current, self.baseline()).map_err(BaselineError::Delta)
    }

    fn latest_tick(&self) -> Option<SnapshotTick> {
        self.pending
            .last_key_value()
            .map(|(&tick, _snapshot)| tick)
            .or_else(|| self.acknowledged_tick())
    }

    fn evict_oldest_pending(&mut self) -> Option<SnapshotTick> {
        if self.pending.len() <= MAX_SENT_SNAPSHOTS {
            return None;
        }
        let oldest = self
            .pending
            .first_key_value()
            .map(|(&tick, _snapshot)| tick)?;
        let _removed = self.pending.remove(&oldest);
        Some(oldest)
    }
}

#[derive(Debug, Clone)]
struct SentSnapshot {
    snapshot: Arc<Snapshot>,
    digest: ProjectionDigest,
}

#[derive(Debug, Clone)]
struct PendingSnapshot {
    baseline: Option<Arc<Snapshot>>,
    delta: SnapshotDelta,
    snapshot: Option<Snapshot>,
    digest: ProjectionDigest,
}

/// Invalid sent-snapshot or applied-ack progression.
#[derive(Debug, Clone, PartialEq, Eq, thiserror::Error)]
pub enum BaselineError {
    /// Sent snapshots were not recorded in increasing tick order.
    #[error("sent snapshot tick {tick} must be newer than {latest}")]
    NonIncreasingSnapshot {
        /// Rejected snapshot tick.
        tick: SnapshotTick,
        /// Latest retained or acknowledged tick.
        latest: SnapshotTick,
    },
    /// A caller supplied a delta for a different snapshot or acknowledged baseline.
    #[error(
        "snapshot {snapshot} delta {delta} uses baseline {actual_baseline:?}, expected {expected_baseline:?}"
    )]
    DeltaDoesNotMatchBaseline {
        /// Snapshot being retained.
        snapshot: SnapshotTick,
        /// Tick represented by the supplied delta.
        delta: SnapshotTick,
        /// Exact currently acknowledged baseline.
        expected_baseline: Option<SnapshotTick>,
        /// Baseline declared by the supplied delta.
        actual_baseline: Option<SnapshotTick>,
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
    /// Tick matched but the client's reconstructed projection digest did not.
    #[error("snapshot acknowledgement digest does not match tick {tick}")]
    DigestMismatch {
        /// Snapshot tick with conflicting digest.
        tick: SnapshotTick,
    },
    /// Canonical snapshot digest construction failed.
    #[error(transparent)]
    Snapshot(#[from] SnapshotError),
    /// Delta construction rejected the selected baseline.
    #[error(transparent)]
    Delta(#[from] DeltaError),
}
