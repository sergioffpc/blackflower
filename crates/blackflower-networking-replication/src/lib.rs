//! Transport-agnostic snapshot replication for sealed authoritative state.
//!
//! Replication first applies revision-stable component visibility, then projects
//! sealed state through a client's stateful [`AoiTracker`]. Component-level
//! operations are built only against the exact applied baseline retained by
//! [`BaselineTracker`].

mod aoi;
mod baseline;
mod delta;
mod quantization;
mod snapshot;
mod types;

pub use aoi::{
    AOI_ENTRY_RADIUS_METERS, AOI_HYSTERESIS_SECONDS, AOI_MINIMUM_HYSTERESIS_METERS, AoiError,
    AoiTracker, Position, ReplicationSource, SourceEntity,
};
pub use baseline::{BaselineError, BaselineTracker, MAX_SENT_SNAPSHOTS};
pub use delta::{
    DeltaError, DeltaOperation, MAX_DELTA_OPERATIONS, SnapshotDelta, build_snapshot_chunks,
};
pub use quantization::{
    POSITION_UNITS_PER_METER, QuantizationError, QuantizedAngle, QuantizedPosition,
    QuantizedQuaternion, QuantizedVelocity, VELOCITY_UNITS_PER_METER_PER_SECOND,
};
pub use snapshot::{
    ComponentState, EntityState, MAX_COMPONENTS_PER_ENTITY, MAX_SNAPSHOT_ENTITIES,
    ProjectionBundle, ProjectionView, SNAPSHOT_REASSEMBLY_DEADLINE, Snapshot, SnapshotBuilder,
    SnapshotError, SnapshotReassembler,
};
pub use types::{
    ComponentDescriptor, ComponentId, ComponentRegistry, ComponentSampleTick, EntityIdAllocator,
    IdentityError, ProjectionKind, RegistryError, ReplicatedEntityId, ReplicationPriority,
    SnapshotTick,
};
