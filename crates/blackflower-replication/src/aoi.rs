use std::collections::{BTreeMap, BTreeSet};

use crate::{ReplicatedEntityId, Snapshot, SnapshotTick};

/// Validated world-space position used by replication interest management.
#[derive(Debug, Clone, Copy, PartialEq)]
pub struct Position([f64; 3]);

impl Position {
    /// Construct a finite world-space position.
    pub fn new(x: f64, y: f64, z: f64) -> Result<Self, AoiError> {
        let coordinates = [x, y, z];
        if coordinates.into_iter().all(f64::is_finite) {
            Ok(Self(coordinates))
        } else {
            Err(AoiError::NonFinitePosition)
        }
    }

    /// Return the position coordinates.
    #[must_use]
    pub const fn coordinates(self) -> [f64; 3] {
        self.0
    }
}

/// One entity exposed by sealed authoritative state to replication.
#[derive(Debug, Clone, PartialEq)]
pub struct SourceEntity<S> {
    id: ReplicatedEntityId,
    position: Position,
    state: S,
}

impl<S> SourceEntity<S> {
    /// Construct one spatially located replication source entity.
    #[must_use]
    pub const fn new(id: ReplicatedEntityId, position: Position, state: S) -> Self {
        Self {
            id,
            position,
            state,
        }
    }

    /// Return the stable protocol identity.
    #[must_use]
    pub const fn id(&self) -> ReplicatedEntityId {
        self.id
    }

    /// Return the authoritative world-space position.
    #[must_use]
    pub const fn position(&self) -> Position {
        self.position
    }

    /// Return the unquantized replication state.
    #[must_use]
    pub const fn state(&self) -> &S {
        &self.state
    }
}

/// Sealed, canonically ordered source state available to replication.
#[derive(Debug, Clone, PartialEq)]
pub struct ReplicationSource<S> {
    tick: SnapshotTick,
    entities: BTreeMap<ReplicatedEntityId, SourceEntity<S>>,
}

impl<S> ReplicationSource<S> {
    /// Build a source view, rejecting duplicate protocol identities.
    pub fn new(
        tick: SnapshotTick,
        entities: impl IntoIterator<Item = SourceEntity<S>>,
    ) -> Result<Self, AoiError> {
        let mut ordered = BTreeMap::new();
        for entity in entities {
            let id = entity.id();
            if ordered.insert(id, entity).is_some() {
                return Err(AoiError::DuplicateEntity { entity: id });
            }
        }
        Ok(Self {
            tick,
            entities: ordered,
        })
    }

    /// Return the authoritative tick represented by this source.
    #[must_use]
    pub const fn tick(&self) -> SnapshotTick {
        self.tick
    }

    /// Iterate source entities in stable protocol-identity order.
    pub fn entities(
        &self,
    ) -> impl ExactSizeIterator<Item = &SourceEntity<S>> + DoubleEndedIterator {
        self.entities.values()
    }
}

/// Spherical area of interest evaluated against sealed entity positions.
#[derive(Debug, Clone, Copy, PartialEq)]
pub struct SphericalAoi {
    center: Position,
    radius: f64,
}

impl SphericalAoi {
    /// Construct a finite area of interest with a non-negative radius.
    pub fn new(center: Position, radius: f64) -> Result<Self, AoiError> {
        if !radius.is_finite() || radius.is_sign_negative() {
            return Err(AoiError::InvalidRadius { radius });
        }
        Ok(Self { center, radius })
    }

    /// Return whether `position` lies inside or on the area boundary.
    #[must_use]
    pub fn contains(self, position: Position) -> bool {
        let [center_x, center_y, center_z] = self.center.coordinates();
        let [position_x, position_y, position_z] = position.coordinates();
        let distance = (position_x - center_x)
            .hypot(position_y - center_y)
            .hypot(position_z - center_z);
        distance <= self.radius
    }

    /// Project sealed source state into one canonically ordered client snapshot.
    #[must_use]
    pub fn project<S: Clone>(
        self,
        source: &ReplicationSource<S>,
        always_relevant: &BTreeSet<ReplicatedEntityId>,
    ) -> Snapshot<S> {
        let entities = source
            .entities
            .iter()
            .filter(|(id, entity)| always_relevant.contains(id) || self.contains(entity.position()))
            .map(|(&id, entity)| (id, entity.state().clone()))
            .collect();
        Snapshot::from_ordered(source.tick(), entities)
    }
}

/// Invalid replication interest-management input.
#[derive(Debug, Clone, Copy, PartialEq, thiserror::Error)]
pub enum AoiError {
    /// At least one position coordinate was not finite.
    #[error("replication position coordinates must be finite")]
    NonFinitePosition,
    /// The radius was negative or not finite.
    #[error("area-of-interest radius must be finite and non-negative, got {radius}")]
    InvalidRadius {
        /// Rejected radius.
        radius: f64,
    },
    /// A source entity identity appeared more than once.
    #[error("replication source entity {entity} appears more than once")]
    DuplicateEntity {
        /// Duplicated protocol identity.
        entity: ReplicatedEntityId,
    },
}
