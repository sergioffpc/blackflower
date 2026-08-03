use std::collections::{BTreeMap, BTreeSet};

use crate::{EntityState, ReplicatedEntityId, Snapshot, SnapshotTick};

/// Fixed spherical AOI entry radius.
pub const AOI_ENTRY_RADIUS_METERS: f64 = 512.0;
/// Minimum exit hysteresis beyond the entry radius.
pub const AOI_MINIMUM_HYSTERESIS_METERS: f64 = 16.0;
/// Time horizon multiplied by maximum speed for dynamic hysteresis.
pub const AOI_HYSTERESIS_SECONDS: f64 = 0.5;

/// Validated world-space position used by replication interest management.
#[derive(Debug, Clone, Copy, PartialEq)]
pub struct Position([f64; 3]);

impl Position {
    /// Construct a finite world-space position in metres.
    pub fn new(x: f64, y: f64, z: f64) -> Result<Self, AoiError> {
        let coordinates = [x, y, z];
        if coordinates.into_iter().all(f64::is_finite) {
            Ok(Self(coordinates))
        } else {
            Err(AoiError::NonFinitePosition)
        }
    }

    /// Return the position coordinates in metres.
    #[must_use]
    pub const fn coordinates(self) -> [f64; 3] {
        self.0
    }

    fn distance(self, other: Self) -> f64 {
        let [self_x, self_y, self_z] = self.0;
        let [other_x, other_y, other_z] = other.0;
        (self_x - other_x)
            .hypot(self_y - other_y)
            .hypot(self_z - other_z)
    }
}

/// One already projected entity exposed by sealed authoritative state.
#[derive(Debug, Clone, PartialEq)]
pub struct SourceEntity {
    id: ReplicatedEntityId,
    position: Position,
    state: EntityState,
}

impl SourceEntity {
    /// Construct one spatially located replication source entity.
    #[must_use]
    pub const fn new(id: ReplicatedEntityId, position: Position, state: EntityState) -> Self {
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

    /// Return the already client-projected component state.
    #[must_use]
    pub const fn state(&self) -> &EntityState {
        &self.state
    }
}

/// Sealed, canonically ordered source state available to AOI projection.
#[derive(Debug, Clone, PartialEq)]
pub struct ReplicationSource {
    tick: SnapshotTick,
    entities: BTreeMap<ReplicatedEntityId, SourceEntity>,
}

impl ReplicationSource {
    /// Build a source view, rejecting duplicate protocol identities.
    pub fn new(
        tick: SnapshotTick,
        entities: impl IntoIterator<Item = SourceEntity>,
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

    /// Iterate source entities in stable protocol identity order.
    pub fn entities(&self) -> impl ExactSizeIterator<Item = &SourceEntity> + DoubleEndedIterator {
        self.entities.values()
    }
}

/// Stateful spherical AOI with fixed entry radius and speed-aware exit hysteresis.
#[derive(Debug, Clone, PartialEq)]
pub struct AoiTracker {
    center: Position,
    maximum_speed_meters_per_second: f64,
    inside: BTreeSet<ReplicatedEntityId>,
}

impl AoiTracker {
    /// Construct stateful AOI with the authoritative maximum relevant speed.
    pub fn new(center: Position, maximum_speed_meters_per_second: f64) -> Result<Self, AoiError> {
        validate_speed(maximum_speed_meters_per_second)?;
        Ok(Self {
            center,
            maximum_speed_meters_per_second,
            inside: BTreeSet::new(),
        })
    }

    /// Move the observer's AOI centre.
    pub const fn set_center(&mut self, center: Position) {
        self.center = center;
    }

    /// Update the authoritative maximum relevant speed.
    pub fn set_maximum_speed(&mut self, meters_per_second: f64) -> Result<(), AoiError> {
        validate_speed(meters_per_second)?;
        self.maximum_speed_meters_per_second = meters_per_second;
        Ok(())
    }

    /// Return the fixed entry radius.
    #[must_use]
    pub const fn entry_radius(&self) -> f64 {
        AOI_ENTRY_RADIUS_METERS
    }

    /// Return `512 m + max(16 m, vmax * 0.5 s)`.
    #[must_use]
    pub fn exit_radius(&self) -> f64 {
        AOI_ENTRY_RADIUS_METERS
            + AOI_MINIMUM_HYSTERESIS_METERS
                .max(self.maximum_speed_meters_per_second * AOI_HYSTERESIS_SECONDS)
    }

    /// Project source state and update retained AOI membership.
    ///
    /// An entity that leaves is forgotten. A later re-entry uses the same
    /// stable identity but appears as a new spawn against the current baseline.
    #[must_use]
    pub fn project(
        &mut self,
        source: &ReplicationSource,
        always_relevant: &BTreeSet<ReplicatedEntityId>,
    ) -> Snapshot {
        let exit_radius = self.exit_radius();
        let mut projected = BTreeMap::new();
        let mut next_inside = BTreeSet::new();
        for (id, entity) in &source.entities {
            let radius = if self.inside.contains(id) {
                exit_radius
            } else {
                AOI_ENTRY_RADIUS_METERS
            };
            let relevant =
                always_relevant.contains(id) || self.center.distance(entity.position()) <= radius;
            if relevant {
                next_inside.insert(*id);
                projected.insert(*id, entity.state().clone());
            }
        }
        self.inside = next_inside;
        Snapshot::from_ordered(source.tick(), projected)
    }
}

/// Invalid replication interest-management input.
#[derive(Debug, Clone, Copy, PartialEq, thiserror::Error)]
pub enum AoiError {
    /// At least one position coordinate was not finite.
    #[error("replication position coordinates must be finite")]
    NonFinitePosition,
    /// Maximum relevant speed was negative or non-finite.
    #[error("AOI maximum speed must be finite and non-negative, got {value}")]
    InvalidMaximumSpeed {
        /// Rejected speed.
        value: f64,
    },
    /// A source entity identity appeared more than once.
    #[error("replication source entity {entity} appears more than once")]
    DuplicateEntity {
        /// Duplicated protocol identity.
        entity: ReplicatedEntityId,
    },
}

fn validate_speed(value: f64) -> Result<(), AoiError> {
    if value.is_finite() && !value.is_sign_negative() {
        Ok(())
    } else {
        Err(AoiError::InvalidMaximumSpeed { value })
    }
}
