use std::collections::{BTreeMap, BTreeSet};

use glam::DVec3;

use crate::{EntityState, ReplicatedEntityId, Snapshot, SnapshotTick};

/// Fixed spherical AOI entry radius.
pub const AOI_ENTRY_RADIUS_METERS: f64 = 512.0;
/// Minimum exit hysteresis beyond the entry radius.
pub const AOI_MINIMUM_HYSTERESIS_METERS: f64 = 16.0;
/// Time horizon multiplied by maximum speed for dynamic hysteresis.
pub const AOI_HYSTERESIS_SECONDS: f64 = 0.5;
const AOI_INDEX_CELL_METERS: f64 = AOI_ENTRY_RADIUS_METERS;

/// Validated world-space position used by replication interest management.
#[derive(Debug, Clone, Copy, PartialEq)]
pub struct Position(DVec3);

impl Position {
    /// Construct a finite world-space position in metres.
    pub fn new(x: f64, y: f64, z: f64) -> Result<Self, AoiError> {
        let coordinates = DVec3::new(x, y, z);
        if coordinates.is_finite() {
            Ok(Self(coordinates))
        } else {
            Err(AoiError::NonFinitePosition)
        }
    }

    /// Return the position coordinates in metres.
    #[must_use]
    pub const fn coordinates(self) -> DVec3 {
        self.0
    }

    fn distance_squared(self, other: Self) -> f64 {
        self.0.distance_squared(other.0)
    }
}

#[derive(Debug, Clone, Copy, PartialEq, Eq, PartialOrd, Ord)]
struct SpatialCell([i64; 3]);

impl SpatialCell {
    fn containing(position: Position) -> Self {
        Self(position.coordinates().to_array().map(cell_coordinate))
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
    spatial_index: BTreeMap<SpatialCell, Vec<ReplicatedEntityId>>,
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
        let mut spatial_index = BTreeMap::<SpatialCell, Vec<ReplicatedEntityId>>::new();
        for entity in ordered.values() {
            spatial_index
                .entry(SpatialCell::containing(entity.position()))
                .or_default()
                .push(entity.id());
        }
        Ok(Self {
            tick,
            entities: ordered,
            spatial_index,
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

    fn collect_candidates(
        &self,
        center: Position,
        radius: f64,
        output: &mut Vec<ReplicatedEntityId>,
    ) {
        output.clear();
        let minimum = SpatialCell(
            (center.coordinates() - DVec3::splat(radius))
                .to_array()
                .map(cell_coordinate),
        );
        let maximum = SpatialCell(
            (center.coordinates() + DVec3::splat(radius))
                .to_array()
                .map(cell_coordinate),
        );
        let widths = std::array::from_fn::<u128, 3, _>(|axis| {
            u128::from(maximum.0[axis].abs_diff(minimum.0[axis])).saturating_add(1)
        });
        let cell_count = widths.into_iter().fold(1_u128, u128::saturating_mul);
        let occupied_count = self.spatial_index.len() as u128;
        if cell_count <= occupied_count {
            for cell_x in minimum.0[0]..=maximum.0[0] {
                for cell_y in minimum.0[1]..=maximum.0[1] {
                    for cell_z in minimum.0[2]..=maximum.0[2] {
                        if let Some(entities) = self
                            .spatial_index
                            .get(&SpatialCell([cell_x, cell_y, cell_z]))
                        {
                            output.extend_from_slice(entities);
                        }
                    }
                }
            }
        } else {
            for (cell, entities) in self.spatial_index.range(minimum..=maximum) {
                if cell.0[1] >= minimum.0[1]
                    && cell.0[1] <= maximum.0[1]
                    && cell.0[2] >= minimum.0[2]
                    && cell.0[2] <= maximum.0[2]
                {
                    output.extend_from_slice(entities);
                }
            }
        }
    }
}

/// Stateful spherical AOI with fixed entry radius and speed-aware exit hysteresis.
#[derive(Debug, Clone, PartialEq)]
pub struct AoiTracker {
    center: Position,
    maximum_speed_meters_per_second: f64,
    inside: BTreeSet<ReplicatedEntityId>,
    next_inside: BTreeSet<ReplicatedEntityId>,
    candidates: Vec<ReplicatedEntityId>,
}

impl AoiTracker {
    /// Construct stateful AOI with the authoritative maximum relevant speed.
    pub fn new(center: Position, maximum_speed_meters_per_second: f64) -> Result<Self, AoiError> {
        validate_speed(maximum_speed_meters_per_second)?;
        Ok(Self {
            center,
            maximum_speed_meters_per_second,
            inside: BTreeSet::new(),
            next_inside: BTreeSet::new(),
            candidates: Vec::new(),
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
        self.next_inside.clear();
        source.collect_candidates(self.center, exit_radius, &mut self.candidates);
        self.candidates.extend(always_relevant.iter().copied());
        self.candidates.sort_unstable();
        self.candidates.dedup();
        for id in self.candidates.iter().copied() {
            let Some(entity) = source.entities.get(&id) else {
                continue;
            };
            let radius = if self.inside.contains(&id) {
                exit_radius
            } else {
                AOI_ENTRY_RADIUS_METERS
            };
            let relevant = always_relevant.contains(&id)
                || self.center.distance_squared(entity.position()) <= radius * radius;
            if relevant {
                self.next_inside.insert(id);
                projected.insert(id, entity.state().clone());
            }
        }
        self.candidates.clear();
        std::mem::swap(&mut self.inside, &mut self.next_inside);
        self.next_inside.clear();
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

#[allow(
    clippy::cast_possible_truncation,
    reason = "flooring a finite world coordinate to an integer grid cell is the intended quantization"
)]
fn cell_coordinate(value: f64) -> i64 {
    (value / AOI_INDEX_CELL_METERS).floor() as i64
}
