use std::collections::BTreeMap;
use std::time::Duration;

use blackflower_networking::{
    DropReason, MAX_SNAPSHOT_CHUNKS, ProjectionDigest, ProtocolRevision, SimulationTick,
    SnapshotAppliedAck, SnapshotChunk, StateBootstrapHeader, record_drop,
};
use blackflower_networking_replication::{
    DeltaError, MAX_SENT_SNAPSHOTS, Snapshot, SnapshotDelta, SnapshotError, SnapshotReassembler,
    SnapshotTick,
};

pub(crate) struct SnapshotInbox {
    pending: BTreeMap<SimulationTick, PendingSnapshot>,
    history: BTreeMap<SnapshotTick, StoredSnapshot>,
    latest: Option<SnapshotTick>,
}

impl SnapshotInbox {
    pub(crate) fn new() -> Self {
        Self {
            pending: BTreeMap::new(),
            history: BTreeMap::new(),
            latest: None,
        }
    }

    pub(crate) fn latest(&self) -> Option<&Snapshot> {
        self.latest
            .and_then(|tick| self.history.get(&tick))
            .map(|stored| &stored.snapshot)
    }

    pub(crate) fn window(&self) -> SnapshotWindow<'_> {
        SnapshotWindow {
            history: &self.history,
        }
    }

    pub(crate) fn bootstrap(
        &mut self,
        header: StateBootstrapHeader,
        body: &[u8],
    ) -> Result<AppliedSnapshot, SnapshotInboxError> {
        let snapshot = Snapshot::decode(body)?;
        validate_bootstrap(header, &snapshot, body.len())?;
        self.pending.clear();
        self.history.clear();
        self.latest = None;
        let applied = applied_snapshot(snapshot, header.projection_digest);
        self.store(applied.clone())?;
        Ok(applied)
    }

    pub(crate) fn ingest_chunk(
        &mut self,
        chunk: SnapshotChunk,
        now: Duration,
    ) -> Result<Option<AppliedSnapshot>, SnapshotInboxError> {
        let metadata = ChunkMetadata::from(&chunk);
        let canonical = if chunk.chunk_count == 1 {
            Some(chunk.payload)
        } else {
            self.push_multi_chunk(chunk, now)?
        };
        let Some(canonical) = canonical else {
            return Ok(None);
        };
        self.apply_delta(metadata, &canonical)
    }

    fn push_multi_chunk(
        &mut self,
        chunk: SnapshotChunk,
        now: Duration,
    ) -> Result<Option<Vec<u8>>, SnapshotInboxError> {
        let tick = chunk.snapshot_tick;
        if let Some(pending) = self.pending.get_mut(&tick) {
            let completed = match pending.reassembler.push(chunk, now) {
                Ok(completed) => completed,
                Err(error @ SnapshotError::ReassemblyExpired) => {
                    drop(self.pending.remove(&tick));
                    record_drop(DropReason::Deadline);
                    return Err(error.into());
                }
                Err(error) => return Err(error.into()),
            };
            if completed.is_some() {
                drop(self.pending.remove(&tick));
            }
            return Ok(completed);
        }
        if self.pending.len() >= MAX_SENT_SNAPSHOTS
            && let Some(oldest) = self.pending.keys().next().copied()
        {
            drop(self.pending.remove(&oldest));
            record_drop(DropReason::Superseded);
        }
        self.pending.insert(
            tick,
            PendingSnapshot {
                reassembler: SnapshotReassembler::new(chunk, now)?,
            },
        );
        Ok(None)
    }

    fn apply_delta(
        &mut self,
        metadata: ChunkMetadata,
        canonical: &[u8],
    ) -> Result<Option<AppliedSnapshot>, SnapshotInboxError> {
        let tick = SnapshotTick::new(metadata.tick.get());
        if self.latest.is_some_and(|latest| tick < latest) {
            record_drop(DropReason::Late);
            return Ok(None);
        }
        if let Some(stored) = self.history.get(&tick) {
            return if stored.digest == metadata.digest {
                Ok(None)
            } else {
                Err(SnapshotInboxError::ConflictingAppliedSnapshot { tick })
            };
        }
        let delta = SnapshotDelta::decode(canonical)?;
        validate_delta_metadata(&delta, metadata)?;
        let baseline = delta.baseline().map(|baseline| {
            self.history
                .get(&baseline)
                .map(|stored| &stored.snapshot)
                .ok_or(SnapshotInboxError::MissingBaseline { tick: baseline })
        });
        let snapshot = delta.apply(baseline.transpose()?)?;
        validate_digest(&snapshot, metadata.digest)?;
        let applied = applied_snapshot(snapshot, metadata.digest);
        self.store(applied.clone())?;
        Ok(Some(applied))
    }

    fn store(&mut self, applied: AppliedSnapshot) -> Result<(), SnapshotInboxError> {
        let tick = applied.snapshot.tick();
        if self.latest.is_some_and(|latest| tick < latest) {
            return Err(SnapshotInboxError::SnapshotRegressed {
                latest: self.latest.unwrap_or(tick),
                next: tick,
            });
        }
        self.history.insert(
            tick,
            StoredSnapshot {
                snapshot: applied.snapshot,
                digest: applied.ack.projection_digest,
            },
        );
        self.latest = Some(tick);
        while self.history.len() > MAX_SENT_SNAPSHOTS {
            let oldest = self.history.keys().next().copied();
            if let Some(oldest) = oldest {
                drop(self.history.remove(&oldest));
            }
        }
        Ok(())
    }
}

/// Immutable chronological window of fully reconstructed authoritative projections.
///
/// The harness retains this bounded history for replication baselines and
/// interpolation. Consumers cannot mutate or extend it.
#[derive(Debug, Clone, Copy)]
pub struct SnapshotWindow<'a> {
    history: &'a BTreeMap<SnapshotTick, StoredSnapshot>,
}

impl<'a> SnapshotWindow<'a> {
    /// Return the number of retained authoritative projections.
    #[must_use]
    pub fn len(self) -> usize {
        self.history.len()
    }

    /// Test whether the interpolation window is empty.
    #[must_use]
    pub fn is_empty(self) -> bool {
        self.history.is_empty()
    }

    /// Return the oldest retained projection.
    #[must_use]
    pub fn oldest(self) -> Option<&'a Snapshot> {
        self.history
            .first_key_value()
            .map(|(_tick, stored)| &stored.snapshot)
    }

    /// Return the newest retained projection.
    #[must_use]
    pub fn newest(self) -> Option<&'a Snapshot> {
        self.history
            .last_key_value()
            .map(|(_tick, stored)| &stored.snapshot)
    }

    /// Iterate over retained projections in authoritative tick order.
    pub fn iter(self) -> impl DoubleEndedIterator<Item = &'a Snapshot> + ExactSizeIterator + 'a {
        self.history.values().map(|stored| &stored.snapshot)
    }
}

#[derive(Debug, Clone)]
pub(crate) struct AppliedSnapshot {
    pub(crate) snapshot: Snapshot,
    pub(crate) ack: SnapshotAppliedAck,
}

struct PendingSnapshot {
    reassembler: SnapshotReassembler,
}

#[derive(Debug)]
struct StoredSnapshot {
    snapshot: Snapshot,
    digest: ProjectionDigest,
}

#[derive(Debug, Clone, Copy)]
struct ChunkMetadata {
    tick: SimulationTick,
    baseline: Option<SimulationTick>,
    digest: ProjectionDigest,
}

impl From<&SnapshotChunk> for ChunkMetadata {
    fn from(chunk: &SnapshotChunk) -> Self {
        Self {
            tick: chunk.snapshot_tick,
            baseline: chunk.baseline_tick,
            digest: chunk.projection_digest,
        }
    }
}

#[derive(Debug, thiserror::Error)]
pub enum SnapshotInboxError {
    #[error(transparent)]
    Snapshot(#[from] SnapshotError),
    #[error(transparent)]
    Delta(#[from] DeltaError),
    #[error("snapshot delta requires unavailable baseline {tick}")]
    MissingBaseline { tick: SnapshotTick },
    #[error("bootstrap metadata does not match its canonical snapshot")]
    BootstrapMismatch,
    #[error("snapshot delta metadata does not match its transport chunks")]
    DeltaMetadataMismatch,
    #[error("snapshot projection digest does not match reconstructed state")]
    DigestMismatch,
    #[error("snapshot tick {next} precedes latest applied tick {latest}")]
    SnapshotRegressed {
        latest: SnapshotTick,
        next: SnapshotTick,
    },
    #[error("snapshot tick {tick} was reused with a different projection")]
    ConflictingAppliedSnapshot { tick: SnapshotTick },
}

fn applied_snapshot(snapshot: Snapshot, digest: ProjectionDigest) -> AppliedSnapshot {
    AppliedSnapshot {
        ack: SnapshotAppliedAck {
            snapshot_tick: SimulationTick::new(snapshot.tick().get()),
            projection_digest: digest,
        },
        snapshot,
    }
}

fn validate_bootstrap(
    header: StateBootstrapHeader,
    snapshot: &Snapshot,
    body_length: usize,
) -> Result<(), SnapshotInboxError> {
    let declared_length = usize::try_from(header.body_length)
        .map_err(|_error| SnapshotInboxError::BootstrapMismatch)?;
    if header.protocol_revision != ProtocolRevision::V1
        || header.snapshot_tick.get() != snapshot.tick().get()
        || declared_length != body_length
    {
        return Err(SnapshotInboxError::BootstrapMismatch);
    }
    validate_digest(snapshot, header.projection_digest)
}

fn validate_delta_metadata(
    delta: &SnapshotDelta,
    metadata: ChunkMetadata,
) -> Result<(), SnapshotInboxError> {
    let baseline = delta.baseline().map(|tick| SimulationTick::new(tick.get()));
    if delta.tick().get() != metadata.tick.get() || baseline != metadata.baseline {
        Err(SnapshotInboxError::DeltaMetadataMismatch)
    } else {
        Ok(())
    }
}

fn validate_digest(
    snapshot: &Snapshot,
    expected: ProjectionDigest,
) -> Result<(), SnapshotInboxError> {
    if snapshot.digest(ProtocolRevision::V1)? == expected {
        Ok(())
    } else {
        Err(SnapshotInboxError::DigestMismatch)
    }
}

const _: () = assert!(MAX_SNAPSHOT_CHUNKS == 4);
