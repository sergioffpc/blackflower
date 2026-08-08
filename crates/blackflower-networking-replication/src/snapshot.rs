use std::collections::BTreeMap;
use std::sync::{Arc, OnceLock};
use std::time::Duration;

use blackflower_networking::{
    MAX_BOOTSTRAP_BYTES, MAX_SNAPSHOT_CHUNKS, ProjectionDigest, ProtocolRevision, SimulationTick,
    SnapshotChunk, projection_digest,
};
use bytes::{BufMut as _, Bytes, BytesMut};

use crate::{
    ComponentId, ComponentRegistry, ComponentSampleTick, ProjectionKind, ReplicatedEntityId,
    ReplicationPriority, SnapshotTick,
};

/// Maximum entities accepted by the v1 canonical snapshot decoder.
pub const MAX_SNAPSHOT_ENTITIES: usize = 4_096;
/// Maximum components accepted on one projected entity.
pub const MAX_COMPONENTS_PER_ENTITY: usize = 256;
/// All chunks of one incremental snapshot must arrive inside this interval.
pub const SNAPSHOT_REASSEMBLY_DEADLINE: Duration = Duration::from_micros(66_700);

/// One full-replacement canonical component sample.
#[derive(Debug, Clone, PartialEq, Eq)]
pub struct ComponentState {
    sample_tick: ComponentSampleTick,
    priority: ReplicationPriority,
    bytes: Bytes,
}

impl ComponentState {
    /// Build a bounded full-replacement component value.
    pub fn new(
        sample_tick: ComponentSampleTick,
        priority: ReplicationPriority,
        bytes: Bytes,
    ) -> Result<Self, SnapshotError> {
        if bytes.len() > usize::from(u16::MAX) {
            return Err(SnapshotError::ComponentTooLarge {
                actual: bytes.len(),
            });
        }
        Ok(Self {
            sample_tick,
            priority,
            bytes,
        })
    }

    /// Return the tick at which this component last changed authoritatively.
    #[must_use]
    pub const fn sample_tick(&self) -> ComponentSampleTick {
        self.sample_tick
    }

    /// Return the normative snapshot scheduling priority.
    #[must_use]
    pub const fn priority(&self) -> ReplicationPriority {
        self.priority
    }

    /// Return canonical full-replacement bytes.
    #[must_use]
    pub fn bytes(&self) -> &[u8] {
        &self.bytes
    }
}

/// Canonically ordered components of one projected entity.
#[derive(Debug, Default, Clone, PartialEq, Eq)]
pub struct EntityState {
    components: Arc<BTreeMap<ComponentId, ComponentState>>,
}

impl EntityState {
    /// Build one entity, rejecting duplicate component IDs.
    pub fn new(
        components: impl IntoIterator<Item = (ComponentId, ComponentState)>,
    ) -> Result<Self, SnapshotError> {
        let mut ordered = BTreeMap::new();
        for (id, state) in components {
            if ordered.insert(id, state).is_some() {
                return Err(SnapshotError::DuplicateComponent { id });
            }
        }
        if ordered.len() > MAX_COMPONENTS_PER_ENTITY {
            return Err(SnapshotError::TooManyComponents {
                actual: ordered.len(),
            });
        }
        Ok(Self {
            components: Arc::new(ordered),
        })
    }

    /// Resolve one component.
    #[must_use]
    pub fn get(&self, id: ComponentId) -> Option<&ComponentState> {
        self.components.get(&id)
    }

    /// Iterate components in stable ID order.
    pub fn components(
        &self,
    ) -> impl ExactSizeIterator<Item = (ComponentId, &ComponentState)> + DoubleEndedIterator {
        self.components.iter().map(|(&id, state)| (id, state))
    }

    pub(crate) fn from_ordered(components: BTreeMap<ComponentId, ComponentState>) -> Self {
        Self {
            components: Arc::new(components),
        }
    }

    pub(crate) fn insert_component(&mut self, id: ComponentId, state: ComponentState) {
        Arc::make_mut(&mut self.components).insert(id, state);
    }

    pub(crate) fn remove_component(&mut self, id: ComponentId) -> Option<ComponentState> {
        Arc::make_mut(&mut self.components).remove(&id)
    }

    pub(crate) fn into_first_component(self) -> Option<(ComponentId, ComponentState)> {
        Arc::unwrap_or_clone(self.components).pop_first()
    }
}

/// Visibility-separated component projections before client projection.
#[derive(Debug, Default, Clone, PartialEq, Eq)]
pub struct ProjectionBundle {
    projections: BTreeMap<ProjectionKind, EntityState>,
}

impl ProjectionBundle {
    /// Add one explicitly classified projection.
    pub fn insert(
        &mut self,
        kind: ProjectionKind,
        state: EntityState,
    ) -> Result<(), SnapshotError> {
        if self.projections.insert(kind, state).is_some() {
            Err(SnapshotError::DuplicateProjection { kind })
        } else {
            Ok(())
        }
    }

    /// Produce the exact client-visible component set before serialization.
    pub fn project(
        &self,
        view: ProjectionView,
        registry: &ComponentRegistry,
    ) -> Result<EntityState, SnapshotError> {
        if registry.revision() != view.protocol_revision {
            return Err(SnapshotError::RegistryRevisionMismatch);
        }
        let mut projected = BTreeMap::new();
        self.merge(ProjectionKind::Public, registry, &mut projected)?;
        if view.owner {
            self.merge(ProjectionKind::Owner, registry, &mut projected)?;
        }
        if view.same_team {
            self.merge(ProjectionKind::Team, registry, &mut projected)?;
        }
        if view.include_global {
            self.merge(ProjectionKind::Global, registry, &mut projected)?;
        }
        Ok(EntityState::from_ordered(projected))
    }

    fn merge(
        &self,
        kind: ProjectionKind,
        registry: &ComponentRegistry,
        output: &mut BTreeMap<ComponentId, ComponentState>,
    ) -> Result<(), SnapshotError> {
        let Some(state) = self.projections.get(&kind) else {
            return Ok(());
        };
        for (id, component) in state.components() {
            validate_component(registry, kind, id, component)?;
            if output.insert(id, component.clone()).is_some() {
                return Err(SnapshotError::DuplicateProjectedComponent { id });
            }
        }
        Ok(())
    }
}

/// Per-observer projection facts.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub struct ProjectionView {
    /// Exact registry revision.
    pub protocol_revision: ProtocolRevision,
    /// Observer controls this entity.
    pub owner: bool,
    /// Observer shares the entity's team.
    pub same_team: bool,
    /// Match-global state is requested explicitly.
    pub include_global: bool,
}

/// Canonically ordered full client projection at one authoritative tick.
#[derive(Debug)]
pub struct Snapshot {
    tick: SnapshotTick,
    entities: BTreeMap<ReplicatedEntityId, EntityState>,
    digest: OnceLock<(ProtocolRevision, ProjectionDigest)>,
}

impl Clone for Snapshot {
    fn clone(&self) -> Self {
        let digest = OnceLock::new();
        if let Some(cached) = self.digest.get() {
            let _already_set = digest.set(*cached);
        }
        Self {
            tick: self.tick,
            entities: self.entities.clone(),
            digest,
        }
    }
}

impl PartialEq for Snapshot {
    fn eq(&self, other: &Self) -> bool {
        self.tick == other.tick && self.entities == other.entities
    }
}

impl Eq for Snapshot {}

/// Mutable construction surface that carries unchanged component sample ticks.
#[derive(Debug, Clone)]
pub struct SnapshotBuilder {
    tick: SnapshotTick,
    entities: BTreeMap<ReplicatedEntityId, EntityState>,
}

impl SnapshotBuilder {
    /// Start an empty full projection at one authoritative tick.
    #[must_use]
    pub const fn new(tick: SnapshotTick) -> Self {
        Self {
            tick,
            entities: BTreeMap::new(),
        }
    }

    /// Carry a previous projection forward unchanged.
    ///
    /// Components not explicitly replaced retain their original `sample_tick`.
    #[must_use]
    pub fn from_previous(tick: SnapshotTick, previous: &Snapshot) -> Self {
        Self {
            tick,
            entities: previous.entities.clone(),
        }
    }

    /// Spawn or replace the complete projected state of one entity.
    pub fn upsert_entity(&mut self, entity: ReplicatedEntityId, state: EntityState) {
        self.entities.insert(entity, state);
    }

    /// Forget one projected entity.
    pub fn forget_entity(&mut self, entity: ReplicatedEntityId) {
        self.entities.remove(&entity);
    }

    /// Replace one component sample in full.
    pub fn update_component(
        &mut self,
        entity: ReplicatedEntityId,
        component: ComponentId,
        state: ComponentState,
    ) -> Result<(), SnapshotError> {
        let entity_state = self
            .entities
            .get_mut(&entity)
            .ok_or(SnapshotError::MissingEntity { entity })?;
        entity_state.insert_component(component, state);
        Ok(())
    }

    /// Remove one component from an existing projected entity.
    pub fn remove_component(
        &mut self,
        entity: ReplicatedEntityId,
        component: ComponentId,
    ) -> Result<(), SnapshotError> {
        let entity_state = self
            .entities
            .get_mut(&entity)
            .ok_or(SnapshotError::MissingEntity { entity })?;
        let _removed = entity_state.remove_component(component);
        Ok(())
    }

    /// Finish the canonical projection.
    pub fn build(self) -> Result<Snapshot, SnapshotError> {
        if self.entities.len() > MAX_SNAPSHOT_ENTITIES {
            return Err(SnapshotError::TooManyEntities {
                actual: self.entities.len(),
            });
        }
        Ok(Snapshot::from_ordered(self.tick, self.entities))
    }
}

impl Snapshot {
    /// Build a snapshot, rejecting duplicate entity identities.
    pub fn new(
        tick: SnapshotTick,
        entities: impl IntoIterator<Item = (ReplicatedEntityId, EntityState)>,
    ) -> Result<Self, SnapshotError> {
        let mut ordered = BTreeMap::new();
        for (entity, state) in entities {
            if ordered.insert(entity, state).is_some() {
                return Err(SnapshotError::DuplicateEntity { entity });
            }
        }
        if ordered.len() > MAX_SNAPSHOT_ENTITIES {
            return Err(SnapshotError::TooManyEntities {
                actual: ordered.len(),
            });
        }
        Ok(Self {
            tick,
            entities: ordered,
            digest: OnceLock::new(),
        })
    }

    /// Return the authoritative tick represented by this snapshot.
    #[must_use]
    pub const fn tick(&self) -> SnapshotTick {
        self.tick
    }

    /// Return the number of replicated entities.
    #[must_use]
    pub fn len(&self) -> usize {
        self.entities.len()
    }

    /// Return whether the snapshot contains no replicated entities.
    #[must_use]
    pub fn is_empty(&self) -> bool {
        self.entities.is_empty()
    }

    /// Return one projected entity.
    #[must_use]
    pub fn get(&self, entity: ReplicatedEntityId) -> Option<&EntityState> {
        self.entities.get(&entity)
    }

    /// Iterate entities in stable protocol identity order.
    pub fn entities(
        &self,
    ) -> impl ExactSizeIterator<Item = (ReplicatedEntityId, &EntityState)> + DoubleEndedIterator
    {
        self.entities.iter().map(|(&entity, state)| (entity, state))
    }

    /// Serialize the canonical projection with explicit little-endian fields.
    pub fn encode(&self) -> Result<Bytes, SnapshotError> {
        let mut bytes = BytesMut::new();
        push_u64(&mut bytes, self.tick.get());
        push_u32(&mut bytes, count_u32(self.entities.len())?);
        for (entity, state) in self.entities() {
            push_u64(&mut bytes, entity.get());
            push_u16(&mut bytes, count_u16(state.components.len())?);
            for (id, component) in state.components() {
                push_component(&mut bytes, id, component)?;
            }
        }
        if bytes.len() > MAX_BOOTSTRAP_BYTES {
            return Err(SnapshotError::EncodedTooLarge {
                actual: bytes.len(),
            });
        }
        Ok(bytes.freeze())
    }

    /// Serialize once and cache the digest of those exact canonical bytes.
    pub fn encode_with_digest(
        &self,
        revision: ProtocolRevision,
    ) -> Result<(Bytes, ProjectionDigest), SnapshotError> {
        let bytes = self.encode()?;
        let digest = projection_digest(revision, SimulationTick::new(self.tick.get()), &bytes);
        let _already_set = self.digest.set((revision, digest));
        Ok((bytes, digest))
    }

    /// Decode one exact canonical projection with bounds checked before allocation.
    pub fn decode(bytes: &[u8]) -> Result<Self, SnapshotError> {
        if bytes.len() > MAX_BOOTSTRAP_BYTES {
            return Err(SnapshotError::EncodedTooLarge {
                actual: bytes.len(),
            });
        }
        let mut decoder = Decoder::new(bytes);
        let tick = SnapshotTick::new(decoder.u64()?);
        let entity_count = decoder.count_u32(MAX_SNAPSHOT_ENTITIES)?;
        let mut entities = BTreeMap::new();
        for _index in 0..entity_count {
            let id = ReplicatedEntityId::try_from_u64(decoder.u64()?)?;
            let component_count = decoder.count_u16(MAX_COMPONENTS_PER_ENTITY)?;
            let state = decode_entity(&mut decoder, component_count)?;
            if entities.insert(id, state).is_some() {
                return Err(SnapshotError::DuplicateEntity { entity: id });
            }
        }
        decoder.finish()?;
        Ok(Self {
            tick,
            entities,
            digest: OnceLock::new(),
        })
    }

    /// Compute the domain-separated digest of the reconstructed canonical bytes.
    pub fn digest(&self, revision: ProtocolRevision) -> Result<ProjectionDigest, SnapshotError> {
        if let Some((cached_revision, digest)) = self.digest.get()
            && *cached_revision == revision
        {
            return Ok(*digest);
        }
        let (_bytes, digest) = self.encode_with_digest(revision)?;
        Ok(digest)
    }

    pub(crate) fn from_ordered(
        tick: SnapshotTick,
        entities: BTreeMap<ReplicatedEntityId, EntityState>,
    ) -> Self {
        Self {
            tick,
            entities,
            digest: OnceLock::new(),
        }
    }

    pub(crate) fn ordered(&self) -> &BTreeMap<ReplicatedEntityId, EntityState> {
        &self.entities
    }
}

/// All-or-nothing bounded snapshot chunk reassembly.
#[derive(Debug, Clone)]
pub struct SnapshotReassembler {
    started_at: Duration,
    tick: SimulationTick,
    baseline_tick: Option<SimulationTick>,
    digest: ProjectionDigest,
    chunk_count: u8,
    chunks: BTreeMap<u8, Bytes>,
}

impl SnapshotReassembler {
    /// Start reassembly from the first received chunk.
    pub fn new(first: SnapshotChunk, now: Duration) -> Result<Self, SnapshotError> {
        validate_chunk(&first)?;
        let mut chunks = BTreeMap::new();
        chunks.insert(first.chunk_index, first.payload);
        Ok(Self {
            started_at: now,
            tick: first.snapshot_tick,
            baseline_tick: first.baseline_tick,
            digest: first.projection_digest,
            chunk_count: first.chunk_count,
            chunks,
        })
    }

    /// Insert a matching chunk and return canonical bytes only when complete.
    pub fn push(
        &mut self,
        chunk: SnapshotChunk,
        now: Duration,
    ) -> Result<Option<Bytes>, SnapshotError> {
        if now
            .checked_sub(self.started_at)
            .is_none_or(|elapsed| elapsed > SNAPSHOT_REASSEMBLY_DEADLINE)
        {
            return Err(SnapshotError::ReassemblyExpired);
        }
        validate_chunk(&chunk)?;
        if chunk.snapshot_tick != self.tick
            || chunk.baseline_tick != self.baseline_tick
            || chunk.projection_digest != self.digest
            || chunk.chunk_count != self.chunk_count
        {
            return Err(SnapshotError::MismatchedChunk);
        }
        match self.chunks.get(&chunk.chunk_index) {
            Some(existing) if existing == &chunk.payload => return Ok(None),
            Some(_existing) => return Err(SnapshotError::ConflictingChunk),
            None => {
                self.chunks.insert(chunk.chunk_index, chunk.payload);
            }
        }
        if self.chunks.len() != usize::from(self.chunk_count) {
            return Ok(None);
        }
        let mut canonical = BytesMut::new();
        for index in 0..self.chunk_count {
            canonical.extend_from_slice(
                self.chunks
                    .get(&index)
                    .ok_or(SnapshotError::MismatchedChunk)?,
            );
        }
        Ok(Some(canonical.freeze()))
    }
}

/// Invalid projected snapshot, canonical codec, or chunk assembly.
#[derive(Debug, Clone, PartialEq, Eq, thiserror::Error)]
pub enum SnapshotError {
    /// An entity identity appeared more than once.
    #[error("replicated entity {entity} appears more than once")]
    DuplicateEntity {
        /// Duplicated protocol identity.
        entity: ReplicatedEntityId,
    },
    /// A builder operation referenced an absent projected entity.
    #[error("snapshot builder references missing entity {entity}")]
    MissingEntity {
        /// Missing entity identity.
        entity: ReplicatedEntityId,
    },
    /// A component identity appeared more than once.
    #[error("component {id:?} appears more than once")]
    DuplicateComponent {
        /// Duplicated component identity.
        id: ComponentId,
    },
    /// One visibility projection was supplied twice.
    #[error("projection {kind:?} appears more than once")]
    DuplicateProjection {
        /// Duplicated projection.
        kind: ProjectionKind,
    },
    /// Two selected projections attempted to provide the same component.
    #[error("component {id:?} appears in multiple selected projections")]
    DuplicateProjectedComponent {
        /// Conflicting component identity.
        id: ComponentId,
    },
    /// Registry and observer protocol revisions differ.
    #[error("component registry revision does not match projection view")]
    RegistryRevisionMismatch,
    /// A component is absent from the stable registry.
    #[error("component {id:?} is not registered")]
    UnregisteredComponent {
        /// Missing component identity.
        id: ComponentId,
    },
    /// A component was placed in a projection different from its registry entry.
    #[error("component {id:?} is in the wrong projection")]
    WrongProjection {
        /// Misclassified component identity.
        id: ComponentId,
    },
    /// Component bytes exceed their registry or wire bound.
    #[error("component payload has {actual} bytes, maximum is {maximum}")]
    ComponentBound {
        /// Actual payload bytes.
        actual: usize,
        /// Registered maximum bytes.
        maximum: usize,
    },
    /// Component bytes cannot fit the u16 canonical length field.
    #[error("component payload has {actual} bytes and cannot be represented")]
    ComponentTooLarge {
        /// Actual payload bytes.
        actual: usize,
    },
    /// Snapshot exceeds the maximum entity count.
    #[error("snapshot has {actual} entities, maximum is 4096")]
    TooManyEntities {
        /// Actual entity count.
        actual: usize,
    },
    /// Entity exceeds the maximum component count.
    #[error("entity has {actual} components, maximum is 256")]
    TooManyComponents {
        /// Actual component count.
        actual: usize,
    },
    /// Canonical snapshot exceeds the 2 MiB full-state bound.
    #[error("canonical snapshot has {actual} bytes, maximum is 2 MiB")]
    EncodedTooLarge {
        /// Actual encoded bytes.
        actual: usize,
    },
    /// Wire input ended before a declared value was complete.
    #[error("canonical snapshot is truncated")]
    Truncated,
    /// Extra bytes follow the canonical snapshot.
    #[error("canonical snapshot has trailing bytes")]
    Trailing,
    /// A reserved or enum field is invalid.
    #[error("canonical snapshot contains an invalid value")]
    InvalidValue,
    /// A count or length cannot be represented.
    #[error("canonical snapshot integer is out of range")]
    IntegerOutOfRange,
    /// A decoded identity was zero.
    #[error(transparent)]
    Identity(#[from] crate::IdentityError),
    /// Snapshot chunk payload maximum is zero.
    #[error("snapshot chunk payload maximum must be non-zero")]
    ZeroChunkPayload,
    /// Essential state requires more than four snapshot chunks.
    #[error("essential snapshot state requires {required} chunks, maximum is four")]
    EssentialStateExceedsChunkBudget {
        /// Required chunk count.
        required: usize,
    },
    /// Chunk index or count is invalid.
    #[error("snapshot chunk position is invalid")]
    InvalidChunk,
    /// Chunk metadata differs from the active assembly.
    #[error("snapshot chunk does not match the active assembly")]
    MismatchedChunk,
    /// Duplicate chunk index carried different bytes.
    #[error("snapshot chunk identity was reused with different bytes")]
    ConflictingChunk,
    /// Reassembly exceeded 66.7 ms.
    #[error("snapshot chunk reassembly deadline expired")]
    ReassemblyExpired,
}

fn validate_component(
    registry: &ComponentRegistry,
    kind: ProjectionKind,
    id: ComponentId,
    component: &ComponentState,
) -> Result<(), SnapshotError> {
    let descriptor = registry
        .descriptor(id)
        .ok_or(SnapshotError::UnregisteredComponent { id })?;
    if descriptor.projection != kind {
        return Err(SnapshotError::WrongProjection { id });
    }
    let maximum = usize::from(descriptor.maximum_bytes);
    if component.bytes.len() > maximum {
        return Err(SnapshotError::ComponentBound {
            actual: component.bytes.len(),
            maximum,
        });
    }
    Ok(())
}

pub(crate) fn push_component(
    bytes: &mut BytesMut,
    id: ComponentId,
    component: &ComponentState,
) -> Result<(), SnapshotError> {
    push_u16(bytes, id.get());
    push_u64(bytes, component.sample_tick.get());
    bytes.put_u8(priority_code(component.priority));
    bytes.put_u8(0);
    push_u16(bytes, count_u16(component.bytes.len())?);
    bytes.extend_from_slice(&component.bytes);
    Ok(())
}

pub(crate) fn decode_entity(
    decoder: &mut Decoder<'_>,
    component_count: usize,
) -> Result<EntityState, SnapshotError> {
    let mut components = BTreeMap::new();
    for _index in 0..component_count {
        let id = ComponentId::try_from_u16(decoder.u16()?)?;
        let sample_tick = ComponentSampleTick::new(decoder.u64()?);
        let priority = decode_priority(decoder.u8()?)?;
        if decoder.u8()? != 0 {
            return Err(SnapshotError::InvalidValue);
        }
        let length = usize::from(decoder.u16()?);
        let state = ComponentState::new(
            sample_tick,
            priority,
            Bytes::copy_from_slice(decoder.take(length)?),
        )?;
        if components.insert(id, state).is_some() {
            return Err(SnapshotError::DuplicateComponent { id });
        }
    }
    Ok(EntityState::from_ordered(components))
}

fn validate_chunk(chunk: &SnapshotChunk) -> Result<(), SnapshotError> {
    if chunk.chunk_count == 0
        || usize::from(chunk.chunk_count) > MAX_SNAPSHOT_CHUNKS
        || chunk.chunk_index >= chunk.chunk_count
    {
        Err(SnapshotError::InvalidChunk)
    } else {
        Ok(())
    }
}

const fn priority_code(priority: ReplicationPriority) -> u8 {
    match priority {
        ReplicationPriority::Lifecycle => 1,
        ReplicationPriority::OwnerCorrection => 2,
        ReplicationPriority::ActiveActor => 3,
        ReplicationPriority::Remaining => 4,
    }
}

fn decode_priority(value: u8) -> Result<ReplicationPriority, SnapshotError> {
    match value {
        1 => Ok(ReplicationPriority::Lifecycle),
        2 => Ok(ReplicationPriority::OwnerCorrection),
        3 => Ok(ReplicationPriority::ActiveActor),
        4 => Ok(ReplicationPriority::Remaining),
        _ => Err(SnapshotError::InvalidValue),
    }
}

fn count_u16(value: usize) -> Result<u16, SnapshotError> {
    u16::try_from(value).map_err(|_error| SnapshotError::IntegerOutOfRange)
}

fn count_u32(value: usize) -> Result<u32, SnapshotError> {
    u32::try_from(value).map_err(|_error| SnapshotError::IntegerOutOfRange)
}

fn push_u16(bytes: &mut BytesMut, value: u16) {
    bytes.extend_from_slice(&value.to_le_bytes());
}

fn push_u32(bytes: &mut BytesMut, value: u32) {
    bytes.extend_from_slice(&value.to_le_bytes());
}

fn push_u64(bytes: &mut BytesMut, value: u64) {
    bytes.extend_from_slice(&value.to_le_bytes());
}

pub(crate) struct Decoder<'a> {
    bytes: &'a [u8],
    cursor: usize,
}

impl<'a> Decoder<'a> {
    pub(crate) const fn new(bytes: &'a [u8]) -> Self {
        Self { bytes, cursor: 0 }
    }

    pub(crate) fn u8(&mut self) -> Result<u8, SnapshotError> {
        let value = *self
            .bytes
            .get(self.cursor)
            .ok_or(SnapshotError::Truncated)?;
        self.cursor += 1;
        Ok(value)
    }

    pub(crate) fn u16(&mut self) -> Result<u16, SnapshotError> {
        let bytes = self.take(2)?;
        Ok(u16::from_le_bytes([bytes[0], bytes[1]]))
    }

    pub(crate) fn u32(&mut self) -> Result<u32, SnapshotError> {
        let bytes = self.take(4)?;
        Ok(u32::from_le_bytes([bytes[0], bytes[1], bytes[2], bytes[3]]))
    }

    pub(crate) fn u64(&mut self) -> Result<u64, SnapshotError> {
        let bytes = self.take(8)?;
        Ok(u64::from_le_bytes([
            bytes[0], bytes[1], bytes[2], bytes[3], bytes[4], bytes[5], bytes[6], bytes[7],
        ]))
    }

    pub(crate) fn remaining(&self) -> usize {
        self.bytes.len().saturating_sub(self.cursor)
    }

    fn count_u16(&mut self, maximum: usize) -> Result<usize, SnapshotError> {
        let count = usize::from(self.u16()?);
        if count > maximum {
            Err(SnapshotError::TooManyComponents { actual: count })
        } else {
            Ok(count)
        }
    }

    fn count_u32(&mut self, maximum: usize) -> Result<usize, SnapshotError> {
        let count =
            usize::try_from(self.u32()?).map_err(|_error| SnapshotError::IntegerOutOfRange)?;
        if count > maximum {
            Err(SnapshotError::TooManyEntities { actual: count })
        } else {
            Ok(count)
        }
    }

    fn take(&mut self, length: usize) -> Result<&'a [u8], SnapshotError> {
        let end = self
            .cursor
            .checked_add(length)
            .ok_or(SnapshotError::IntegerOutOfRange)?;
        let value = self
            .bytes
            .get(self.cursor..end)
            .ok_or(SnapshotError::Truncated)?;
        self.cursor = end;
        Ok(value)
    }

    pub(crate) fn finish(self) -> Result<(), SnapshotError> {
        if self.cursor == self.bytes.len() {
            Ok(())
        } else {
            Err(SnapshotError::Trailing)
        }
    }
}
