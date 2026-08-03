use std::collections::BTreeMap;

use blackflower_networking::{
    MAX_BOOTSTRAP_BYTES, MAX_SNAPSHOT_CHUNKS, ProtocolRevision, SimulationTick, SnapshotChunk,
};

use crate::snapshot::{Decoder, decode_entity, push_component};
use crate::{
    ComponentId, ComponentState, EntityState, MAX_COMPONENTS_PER_ENTITY, ReplicatedEntityId,
    ReplicationPriority, Snapshot, SnapshotTick,
};

/// Maximum decoded component operations in one incremental snapshot.
pub const MAX_DELTA_OPERATIONS: usize = 65_536;
const MIN_DELTA_OPERATION_BYTES: usize = 12;

/// One canonical component-level replication operation.
#[derive(Debug, Clone, PartialEq, Eq)]
pub enum DeltaOperation {
    /// Introduce an entity with its complete projected component set.
    Spawn {
        /// Stable session identity, reused when the entity re-enters AOI.
        entity: ReplicatedEntityId,
        /// Complete projected state at spawn time.
        state: EntityState,
    },
    /// Replace one component value in full.
    Update {
        /// Existing projected entity.
        entity: ReplicatedEntityId,
        /// Stable component identity.
        component: ComponentId,
        /// Full replacement value and its source sample tick.
        state: ComponentState,
    },
    /// Remove one component while retaining the projected entity.
    RemoveComponent {
        /// Existing projected entity.
        entity: ReplicatedEntityId,
        /// Component no longer present in this projection.
        component: ComponentId,
    },
    /// Forget an entity that left AOI or otherwise lost relevance.
    Forget {
        /// Entity removed from the client projection.
        entity: ReplicatedEntityId,
    },
}

impl DeltaOperation {
    /// Return the normative scheduling priority of this operation.
    #[must_use]
    pub const fn priority(&self) -> ReplicationPriority {
        match self {
            Self::Spawn { .. } | Self::RemoveComponent { .. } | Self::Forget { .. } => {
                ReplicationPriority::Lifecycle
            }
            Self::Update { state, .. } => state.priority(),
        }
    }
}

/// Canonically ordered operations from an acknowledged baseline to one snapshot.
#[derive(Debug, Clone, PartialEq, Eq)]
pub struct SnapshotDelta {
    tick: SnapshotTick,
    baseline: Option<SnapshotTick>,
    operations: Vec<DeltaOperation>,
}

impl SnapshotDelta {
    /// Compare one full projected snapshot with an optional acknowledged baseline.
    pub fn between(current: &Snapshot, baseline: Option<&Snapshot>) -> Result<Self, DeltaError> {
        let operations = match baseline {
            Some(baseline) => operations_from_baseline(current, baseline)?,
            None => full_operations(current),
        };
        Ok(Self {
            tick: current.tick(),
            baseline: baseline.map(Snapshot::tick),
            operations,
        })
    }

    /// Reconstruct the current snapshot from this delta and exact baseline.
    pub fn apply(&self, baseline: Option<&Snapshot>) -> Result<Snapshot, DeltaError> {
        let mut entities = self.baseline_entities(baseline)?;
        for operation in &self.operations {
            apply_operation(&mut entities, operation)?;
        }
        Ok(Snapshot::from_ordered(self.tick, entities))
    }

    /// Return the authoritative tick produced by this delta.
    #[must_use]
    pub const fn tick(&self) -> SnapshotTick {
        self.tick
    }

    /// Return the exact acknowledged baseline tick, if any.
    #[must_use]
    pub const fn baseline(&self) -> Option<SnapshotTick> {
        self.baseline
    }

    /// Return canonical component-level operations.
    #[must_use]
    pub fn operations(&self) -> &[DeltaOperation] {
        &self.operations
    }

    /// Encode canonical component operations using explicit little-endian fields.
    pub fn encode(&self) -> Result<Vec<u8>, DeltaError> {
        let mut bytes = Vec::new();
        push_u64(&mut bytes, self.tick.get());
        bytes.push(u8::from(self.baseline.is_some()));
        bytes.extend_from_slice(&[0; 3]);
        push_u64(&mut bytes, self.baseline.map_or(0, SnapshotTick::get));
        push_u32(&mut bytes, count_u32(self.operations.len())?);
        for operation in &self.operations {
            encode_operation(&mut bytes, operation)?;
        }
        if bytes.len() > MAX_BOOTSTRAP_BYTES {
            return Err(DeltaError::EncodedTooLarge {
                actual: bytes.len(),
            });
        }
        Ok(bytes)
    }

    /// Decode one exact bounded canonical component delta.
    pub fn decode(bytes: &[u8]) -> Result<Self, DeltaError> {
        if bytes.len() > MAX_BOOTSTRAP_BYTES {
            return Err(DeltaError::EncodedTooLarge {
                actual: bytes.len(),
            });
        }
        let mut decoder = Decoder::new(bytes);
        let tick = SnapshotTick::new(decoder.u64()?);
        let has_baseline = decode_bool(decoder.u8()?)?;
        if decoder.u8()? != 0 || decoder.u16()? != 0 {
            return Err(DeltaError::Reserved);
        }
        let baseline_value = decoder.u64()?;
        if !has_baseline && baseline_value != 0 {
            return Err(DeltaError::Reserved);
        }
        let count =
            usize::try_from(decoder.u32()?).map_err(|_error| DeltaError::IntegerOutOfRange)?;
        if count > MAX_DELTA_OPERATIONS {
            return Err(DeltaError::TooManyOperations { actual: count });
        }
        if count > decoder.remaining() / MIN_DELTA_OPERATION_BYTES {
            return Err(crate::SnapshotError::Truncated.into());
        }
        let mut operations = Vec::with_capacity(count);
        for _index in 0..count {
            operations.push(decode_operation(&mut decoder)?);
        }
        decoder.finish()?;
        Ok(Self {
            tick,
            baseline: has_baseline.then(|| SnapshotTick::new(baseline_value)),
            operations,
        })
    }

    fn baseline_entities(
        &self,
        baseline: Option<&Snapshot>,
    ) -> Result<BTreeMap<ReplicatedEntityId, EntityState>, DeltaError> {
        match (self.baseline, baseline) {
            (None, None) => Ok(BTreeMap::new()),
            (None, Some(actual)) => Err(DeltaError::UnexpectedBaseline {
                actual: actual.tick(),
            }),
            (Some(expected), None) => Err(DeltaError::MissingBaseline { expected }),
            (Some(expected), Some(actual)) if expected != actual.tick() => {
                Err(DeltaError::BaselineMismatch {
                    expected,
                    actual: actual.tick(),
                })
            }
            (Some(_expected), Some(actual)) => Ok(actual.ordered().clone()),
        }
    }
}

/// Invalid snapshot delta construction or application.
#[derive(Debug, Clone, PartialEq, Eq, thiserror::Error)]
pub enum DeltaError {
    /// A baseline did not precede the current snapshot.
    #[error("baseline tick {baseline} must precede current snapshot tick {current}")]
    BaselineNotOlder {
        /// Rejected baseline tick.
        baseline: SnapshotTick,
        /// Current snapshot tick.
        current: SnapshotTick,
    },
    /// Applying a delta required a baseline that was not provided.
    #[error("snapshot delta requires baseline tick {expected}")]
    MissingBaseline {
        /// Required baseline tick.
        expected: SnapshotTick,
    },
    /// A full snapshot delta was given an unnecessary baseline.
    #[error("full snapshot delta does not accept baseline tick {actual}")]
    UnexpectedBaseline {
        /// Unexpected baseline tick.
        actual: SnapshotTick,
    },
    /// The supplied baseline did not match the delta header.
    #[error("snapshot delta requires baseline tick {expected}, got {actual}")]
    BaselineMismatch {
        /// Required baseline tick.
        expected: SnapshotTick,
        /// Supplied baseline tick.
        actual: SnapshotTick,
    },
    /// Spawn collided with an existing baseline entity.
    #[error("spawn references an existing entity {entity}")]
    ExistingSpawn {
        /// Existing entity.
        entity: ReplicatedEntityId,
    },
    /// Operation referenced an entity absent from the reconstructed projection.
    #[error("operation references missing entity {entity}")]
    MissingEntity {
        /// Missing entity.
        entity: ReplicatedEntityId,
    },
    /// Removal referenced a component absent from the reconstructed entity.
    #[error("operation removes missing component {component:?} from {entity}")]
    MissingComponent {
        /// Existing entity.
        entity: ReplicatedEntityId,
        /// Missing component.
        component: ComponentId,
    },
    /// Canonical delta exceeds the full-state safety bound.
    #[error("canonical delta has {actual} bytes, maximum is 2 MiB")]
    EncodedTooLarge {
        /// Actual canonical bytes.
        actual: usize,
    },
    /// Decoded operation count exceeds the fixed safety bound.
    #[error("canonical delta has {actual} operations, maximum is 65536")]
    TooManyOperations {
        /// Declared operation count.
        actual: usize,
    },
    /// Reserved canonical bytes were non-zero.
    #[error("canonical delta has non-zero reserved bytes")]
    Reserved,
    /// Canonical delta contains an unknown operation tag.
    #[error("canonical delta has unknown operation tag {0}")]
    UnknownOperation(u8),
    /// Canonical delta contains an invalid boolean.
    #[error("canonical delta contains an invalid boolean")]
    InvalidBoolean,
    /// Count or length cannot be represented.
    #[error("canonical delta integer is out of range")]
    IntegerOutOfRange,
    /// Canonical snapshot component codec failed.
    #[error(transparent)]
    Snapshot(#[from] crate::SnapshotError),
    /// Delta and resulting full projection represent different ticks.
    #[error("delta tick {delta} does not match resulting snapshot tick {snapshot}")]
    TickMismatch {
        /// Delta tick.
        delta: SnapshotTick,
        /// Resulting projection tick.
        snapshot: SnapshotTick,
    },
    /// Chunk payload bound cannot be zero.
    #[error("snapshot chunk payload maximum must be non-zero")]
    ZeroChunkPayload,
    /// Incremental delta requires more than four chunks.
    #[error("snapshot delta requires {required} chunks, maximum is four")]
    ChunkBudgetExceeded {
        /// Required chunk count.
        required: usize,
    },
}

/// Split one component delta into at most four all-or-nothing DATAGRAM chunks.
pub fn build_snapshot_chunks(
    delta: &SnapshotDelta,
    resulting_snapshot: &Snapshot,
    revision: ProtocolRevision,
    maximum_chunk_payload: usize,
) -> Result<Vec<SnapshotChunk>, DeltaError> {
    if delta.tick() != resulting_snapshot.tick() {
        return Err(DeltaError::TickMismatch {
            delta: delta.tick(),
            snapshot: resulting_snapshot.tick(),
        });
    }
    if maximum_chunk_payload == 0 {
        return Err(DeltaError::ZeroChunkPayload);
    }
    let canonical = delta.encode()?;
    let chunk_count = canonical.len().div_ceil(maximum_chunk_payload).max(1);
    if chunk_count > MAX_SNAPSHOT_CHUNKS {
        return Err(DeltaError::ChunkBudgetExceeded {
            required: chunk_count,
        });
    }
    let digest = resulting_snapshot.digest(revision)?;
    let count = u8::try_from(chunk_count).map_err(|_error| DeltaError::IntegerOutOfRange)?;
    canonical
        .chunks(maximum_chunk_payload)
        .enumerate()
        .map(|(index, payload)| {
            Ok(SnapshotChunk {
                snapshot_tick: SimulationTick::new(delta.tick().get()),
                baseline_tick: delta.baseline().map(|tick| SimulationTick::new(tick.get())),
                projection_digest: digest,
                chunk_index: u8::try_from(index).map_err(|_error| DeltaError::IntegerOutOfRange)?,
                chunk_count: count,
                payload: payload.to_vec(),
            })
        })
        .collect()
}

fn operations_from_baseline(
    current: &Snapshot,
    baseline: &Snapshot,
) -> Result<Vec<DeltaOperation>, DeltaError> {
    if baseline.tick() >= current.tick() {
        return Err(DeltaError::BaselineNotOlder {
            baseline: baseline.tick(),
            current: current.tick(),
        });
    }
    let mut operations = Vec::new();
    for (entity, old_state) in baseline.entities() {
        let Some(new_state) = current.get(entity) else {
            operations.push(DeltaOperation::Forget { entity });
            continue;
        };
        for (component, _old) in old_state.components() {
            if new_state.get(component).is_none() {
                operations.push(DeltaOperation::RemoveComponent { entity, component });
            }
        }
    }
    for (entity, new_state) in current.entities() {
        let Some(old_state) = baseline.get(entity) else {
            operations.push(DeltaOperation::Spawn {
                entity,
                state: new_state.clone(),
            });
            continue;
        };
        for (component, new_component) in new_state.components() {
            if old_state.get(component) != Some(new_component) {
                operations.push(DeltaOperation::Update {
                    entity,
                    component,
                    state: new_component.clone(),
                });
            }
        }
    }
    operations.sort_by_key(operation_order);
    Ok(operations)
}

fn full_operations(current: &Snapshot) -> Vec<DeltaOperation> {
    current
        .entities()
        .map(|(entity, state)| DeltaOperation::Spawn {
            entity,
            state: state.clone(),
        })
        .collect()
}

fn operation_order(
    operation: &DeltaOperation,
) -> (ReplicationPriority, ReplicatedEntityId, u8, u16) {
    match operation {
        DeltaOperation::Spawn { entity, .. } => (ReplicationPriority::Lifecycle, *entity, 1, 0),
        DeltaOperation::Update {
            entity,
            component,
            state,
        } => (state.priority(), *entity, 2, component.get()),
        DeltaOperation::RemoveComponent { entity, component } => {
            (ReplicationPriority::Lifecycle, *entity, 3, component.get())
        }
        DeltaOperation::Forget { entity } => (ReplicationPriority::Lifecycle, *entity, 4, 0),
    }
}

fn apply_operation(
    entities: &mut BTreeMap<ReplicatedEntityId, EntityState>,
    operation: &DeltaOperation,
) -> Result<(), DeltaError> {
    match operation {
        DeltaOperation::Spawn { entity, state } => {
            if entities.insert(*entity, state.clone()).is_some() {
                Err(DeltaError::ExistingSpawn { entity: *entity })
            } else {
                Ok(())
            }
        }
        DeltaOperation::Update {
            entity,
            component,
            state,
        } => {
            let entity_state = entities
                .get_mut(entity)
                .ok_or(DeltaError::MissingEntity { entity: *entity })?;
            entity_state.components.insert(*component, state.clone());
            Ok(())
        }
        DeltaOperation::RemoveComponent { entity, component } => {
            let entity_state = entities
                .get_mut(entity)
                .ok_or(DeltaError::MissingEntity { entity: *entity })?;
            if entity_state.components.remove(component).is_none() {
                Err(DeltaError::MissingComponent {
                    entity: *entity,
                    component: *component,
                })
            } else {
                Ok(())
            }
        }
        DeltaOperation::Forget { entity } => {
            if entities.remove(entity).is_none() {
                Err(DeltaError::MissingEntity { entity: *entity })
            } else {
                Ok(())
            }
        }
    }
}

fn encode_operation(bytes: &mut Vec<u8>, operation: &DeltaOperation) -> Result<(), DeltaError> {
    match operation {
        DeltaOperation::Spawn { entity, state } => {
            push_operation_header(bytes, 1, *entity);
            push_u16(bytes, count_u16(state.components().len())?);
            for (id, component) in state.components() {
                push_component(bytes, id, component)?;
            }
        }
        DeltaOperation::Update {
            entity,
            component,
            state,
        } => {
            push_operation_header(bytes, 2, *entity);
            push_component(bytes, *component, state)?;
        }
        DeltaOperation::RemoveComponent { entity, component } => {
            push_operation_header(bytes, 3, *entity);
            push_u16(bytes, component.get());
        }
        DeltaOperation::Forget { entity } => push_operation_header(bytes, 4, *entity),
    }
    Ok(())
}

fn decode_operation(decoder: &mut Decoder<'_>) -> Result<DeltaOperation, DeltaError> {
    let kind = decoder.u8()?;
    if decoder.u8()? != 0 || decoder.u16()? != 0 {
        return Err(DeltaError::Reserved);
    }
    let entity =
        ReplicatedEntityId::try_from_u64(decoder.u64()?).map_err(crate::SnapshotError::from)?;
    match kind {
        1 => {
            let count = usize::from(decoder.u16()?);
            if count > MAX_COMPONENTS_PER_ENTITY {
                return Err(DeltaError::Snapshot(
                    crate::SnapshotError::TooManyComponents { actual: count },
                ));
            }
            Ok(DeltaOperation::Spawn {
                entity,
                state: decode_entity(decoder, count)?,
            })
        }
        2 => {
            let mut components = decode_entity(decoder, 1)?.components;
            let (component, state) = components
                .pop_first()
                .ok_or(DeltaError::IntegerOutOfRange)?;
            Ok(DeltaOperation::Update {
                entity,
                component,
                state,
            })
        }
        3 => Ok(DeltaOperation::RemoveComponent {
            entity,
            component: ComponentId::try_from_u16(decoder.u16()?)
                .map_err(crate::SnapshotError::from)?,
        }),
        4 => Ok(DeltaOperation::Forget { entity }),
        value => Err(DeltaError::UnknownOperation(value)),
    }
}

fn push_operation_header(bytes: &mut Vec<u8>, kind: u8, entity: ReplicatedEntityId) {
    bytes.push(kind);
    bytes.extend_from_slice(&[0; 3]);
    push_u64(bytes, entity.get());
}

fn decode_bool(value: u8) -> Result<bool, DeltaError> {
    match value {
        0 => Ok(false),
        1 => Ok(true),
        _ => Err(DeltaError::InvalidBoolean),
    }
}

fn count_u16(value: usize) -> Result<u16, DeltaError> {
    u16::try_from(value).map_err(|_error| DeltaError::IntegerOutOfRange)
}

fn count_u32(value: usize) -> Result<u32, DeltaError> {
    u32::try_from(value).map_err(|_error| DeltaError::IntegerOutOfRange)
}

fn push_u16(bytes: &mut Vec<u8>, value: u16) {
    bytes.extend_from_slice(&value.to_le_bytes());
}

fn push_u32(bytes: &mut Vec<u8>, value: u32) {
    bytes.extend_from_slice(&value.to_le_bytes());
}

fn push_u64(bytes: &mut Vec<u8>, value: u64) {
    bytes.extend_from_slice(&value.to_le_bytes());
}
