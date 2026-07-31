use std::collections::BTreeMap;

use crate::{ReplicatedEntityId, SnapshotTick};

/// Canonically ordered entity state for one client snapshot.
#[derive(Debug, Clone, PartialEq, Eq)]
pub struct Snapshot<S> {
    tick: SnapshotTick,
    entities: BTreeMap<ReplicatedEntityId, S>,
}

impl<S> Snapshot<S> {
    /// Build a snapshot, rejecting duplicate entity identities.
    pub fn new(
        tick: SnapshotTick,
        entities: impl IntoIterator<Item = (ReplicatedEntityId, S)>,
    ) -> Result<Self, SnapshotError> {
        let mut ordered = BTreeMap::new();
        for (entity, state) in entities {
            if ordered.insert(entity, state).is_some() {
                return Err(SnapshotError::DuplicateEntity { entity });
            }
        }
        Ok(Self {
            tick,
            entities: ordered,
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

    /// Return one entity's state.
    #[must_use]
    pub fn get(&self, entity: ReplicatedEntityId) -> Option<&S> {
        self.entities.get(&entity)
    }

    /// Iterate entities in stable protocol-identity order.
    pub fn entities(
        &self,
    ) -> impl ExactSizeIterator<Item = (ReplicatedEntityId, &S)> + DoubleEndedIterator {
        self.entities.iter().map(|(&entity, state)| (entity, state))
    }

    /// Transform every entity state while preserving tick and canonical order.
    pub fn try_map<T, E>(
        &self,
        mut transform: impl FnMut(ReplicatedEntityId, &S) -> Result<T, E>,
    ) -> Result<Snapshot<T>, E> {
        let mut transformed = BTreeMap::new();
        for (&entity, state) in &self.entities {
            transformed.insert(entity, transform(entity, state)?);
        }
        Ok(Snapshot::from_ordered(self.tick, transformed))
    }

    pub(crate) fn from_ordered(
        tick: SnapshotTick,
        entities: BTreeMap<ReplicatedEntityId, S>,
    ) -> Self {
        Self { tick, entities }
    }

    pub(crate) const fn ordered(&self) -> &BTreeMap<ReplicatedEntityId, S> {
        &self.entities
    }
}

/// Invalid client snapshot construction.
#[derive(Debug, Clone, Copy, PartialEq, Eq, thiserror::Error)]
pub enum SnapshotError {
    /// An entity identity appeared more than once.
    #[error("replicated entity {entity} appears more than once")]
    DuplicateEntity {
        /// Duplicated protocol identity.
        entity: ReplicatedEntityId,
    },
}
