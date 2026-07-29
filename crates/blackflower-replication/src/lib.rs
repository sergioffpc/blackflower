//! Transport-agnostic snapshot replication for sealed authoritative state.
//!
//! Replication first projects a [`ReplicationSource`] through a client's
//! [`SphericalAoi`]. The caller then quantizes its component schema before
//! creating a [`SnapshotDelta`] against the baseline retained by
//! [`BaselineTracker`].

mod aoi;
mod baseline;
mod delta;
mod quantization;
mod snapshot;
mod types;

pub use aoi::{AoiError, Position, ReplicationSource, SourceEntity, SphericalAoi};
pub use baseline::{BaselineError, BaselineTracker};
pub use delta::{DeltaError, EntityUpdate, SnapshotDelta};
pub use quantization::{
    PositionQuantizer, QuantizationError, QuantizedPosition, QuantizedScalar, ScalarQuantizer,
};
pub use snapshot::{Snapshot, SnapshotError};
pub use types::{ReplicatedEntityId, SnapshotTick};
