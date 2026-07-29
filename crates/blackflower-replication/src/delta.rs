use std::collections::BTreeMap;

use crate::{ReplicatedEntityId, Snapshot, SnapshotTick};

/// New or changed state for one replicated entity.
#[derive(Debug, Clone, PartialEq, Eq)]
pub struct EntityUpdate<S> {
    entity: ReplicatedEntityId,
    state: S,
}

impl<S> EntityUpdate<S> {
    /// Return the updated entity identity.
    #[must_use]
    pub const fn entity(&self) -> ReplicatedEntityId {
        self.entity
    }

    /// Return the replacement quantized state.
    #[must_use]
    pub const fn state(&self) -> &S {
        &self.state
    }
}

/// Canonically ordered changes from an acknowledged baseline to one snapshot.
#[derive(Debug, Clone, PartialEq, Eq)]
pub struct SnapshotDelta<S> {
    tick: SnapshotTick,
    baseline: Option<SnapshotTick>,
    removed: Vec<ReplicatedEntityId>,
    updates: Vec<EntityUpdate<S>>,
}

impl<S: Clone + Eq> SnapshotDelta<S> {
    /// Compare a quantized snapshot with an optional acknowledged baseline.
    pub fn between(
        current: &Snapshot<S>,
        baseline: Option<&Snapshot<S>>,
    ) -> Result<Self, DeltaError> {
        match baseline {
            Some(baseline) => Self::from_baseline(current, baseline),
            None => Ok(Self::full(current)),
        }
    }

    /// Reconstruct the current snapshot from this delta and its baseline.
    pub fn apply(&self, baseline: Option<&Snapshot<S>>) -> Result<Snapshot<S>, DeltaError> {
        let mut entities = self.baseline_entities(baseline)?;
        for entity in &self.removed {
            if entities.remove(entity).is_none() {
                return Err(DeltaError::MissingRemovedEntity { entity: *entity });
            }
        }
        for update in &self.updates {
            entities.insert(update.entity(), update.state().clone());
        }
        Ok(Snapshot::from_ordered(self.tick, entities))
    }

    fn full(current: &Snapshot<S>) -> Self {
        let updates = current
            .entities()
            .map(|(entity, state)| EntityUpdate {
                entity,
                state: state.clone(),
            })
            .collect();
        Self {
            tick: current.tick(),
            baseline: None,
            removed: Vec::new(),
            updates,
        }
    }

    fn from_baseline(current: &Snapshot<S>, baseline: &Snapshot<S>) -> Result<Self, DeltaError> {
        if baseline.tick() >= current.tick() {
            return Err(DeltaError::BaselineNotOlder {
                baseline: baseline.tick(),
                current: current.tick(),
            });
        }
        let removed = baseline
            .entities()
            .filter_map(|(entity, _state)| current.get(entity).is_none().then_some(entity))
            .collect();
        let updates = current
            .entities()
            .filter(|(entity, state)| baseline.get(*entity) != Some(*state))
            .map(|(entity, state)| EntityUpdate {
                entity,
                state: state.clone(),
            })
            .collect();
        Ok(Self {
            tick: current.tick(),
            baseline: Some(baseline.tick()),
            removed,
            updates,
        })
    }

    fn baseline_entities(
        &self,
        baseline: Option<&Snapshot<S>>,
    ) -> Result<BTreeMap<ReplicatedEntityId, S>, DeltaError> {
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

impl<S> SnapshotDelta<S> {
    /// Return the authoritative tick produced by this delta.
    #[must_use]
    pub const fn tick(&self) -> SnapshotTick {
        self.tick
    }

    /// Return the acknowledged baseline tick, or `None` for a full snapshot.
    #[must_use]
    pub const fn baseline(&self) -> Option<SnapshotTick> {
        self.baseline
    }

    /// Return entities that left the client's area of interest.
    #[must_use]
    pub fn removed(&self) -> &[ReplicatedEntityId] {
        &self.removed
    }

    /// Return new and changed entities in protocol-identity order.
    #[must_use]
    pub fn updates(&self) -> &[EntityUpdate<S>] {
        &self.updates
    }
}

/// Invalid snapshot delta construction or application.
#[derive(Debug, Clone, Copy, PartialEq, Eq, thiserror::Error)]
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
    /// A removal referred to an entity absent from the baseline.
    #[error("snapshot delta removes entity {entity}, but the baseline does not contain it")]
    MissingRemovedEntity {
        /// Missing baseline entity.
        entity: ReplicatedEntityId,
    },
}
