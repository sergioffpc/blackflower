use blackflower_math::{Quat, components::Transform};
use blackflower_protocol::EntityDelta;
use hashbrown::{HashMap, HashSet};
use hecs::{Entity, World};
use serde::{Deserialize, Serialize};
use tracing::{error, info, trace, warn};

pub mod arena;
pub mod presentation;
pub mod replication;
pub mod simulation;

#[repr(transparent)]
#[derive(Clone, Copy, Debug, Default, PartialEq, Eq, Hash, Serialize, Deserialize)]
pub struct EntityId(u64);

impl EntityId {
    pub const NONE: Self = Self(0);

    #[must_use]
    pub const fn is_none(self) -> bool {
        self.0 == 0
    }
}

impl From<u64> for EntityId {
    fn from(value: u64) -> Self {
        Self(value)
    }
}

impl From<EntityId> for u64 {
    fn from(value: EntityId) -> Self {
        value.0
    }
}

impl std::fmt::Display for EntityId {
    fn fmt(&self, f: &mut std::fmt::Formatter<'_>) -> std::fmt::Result {
        write!(f, "{}", self.0)
    }
}

#[derive(Debug, Default)]
pub struct EntityIdAllocator {
    next: u64,
}

impl EntityIdAllocator {
    #[must_use]
    pub const fn new() -> Self {
        Self { next: 0 }
    }

    pub const fn allocate(&mut self) -> EntityId {
        self.next += 1;
        EntityId(self.next)
    }
}

#[derive(Default)]
pub struct Entities {
    world: World,

    #[allow(clippy::struct_field_names)]
    entities: HashMap<EntityId, Entity>,
}

impl Entities {
    /// Apply a full-snapshot delta (no baseline): the delta carries every
    /// field, so `merge_delta(None, ..)` resolves the transform — and skips the
    /// entity if a field is missing (a server bug), rather than defaulting to a
    /// degenerate origin/zero-rotation.
    pub fn apply(&mut self, k: EntityId, v: &EntityDelta) -> Option<Transform> {
        self.apply_delta(k, v, None)
    }

    pub fn keys(&self) -> impl Iterator<Item = EntityId> + '_ {
        self.entities.keys().copied()
    }

    /// Apply an incremental delta against the entity's current transform.
    pub fn merge(&mut self, k: EntityId, v: &EntityDelta) -> Option<Transform> {
        let current = self.transform_of(k);
        self.apply_delta(k, v, current)
    }

    /// Shared delta application. Upserts the transform only when the delta
    /// resolves one (skipping unknown entities with an incomplete delta).
    /// Returns the resolved transform, or `None` when the entity was skipped.
    /// Property deltas are ignored client-side — nothing renders them yet.
    fn apply_delta(
        &mut self,
        k: EntityId,
        v: &EntityDelta,
        current: Option<Transform>,
    ) -> Option<Transform> {
        let transform = merge_delta(current, v);
        if let Some(t) = transform {
            self.upsert_entity(k, t);
        } else {
            warn!(id = %k, "delta has no transform for unknown entity — skipped");
        }
        transform
    }

    pub fn remove(&mut self, k: &EntityId) {
        if let Some(entity) = self.entities.remove(k)
            && let Err(e) = self.world.despawn(entity)
        {
            warn!(error = %e, id = %k, "failed to despawn entity");
        }
    }

    pub fn retain_present_entities(&mut self, present: &HashSet<EntityId>) {
        self.entities.retain(|id, entity| {
            if present.contains(id) {
                true
            } else {
                #[allow(clippy::excessive_nesting)]
                if let Err(e) = self.world.despawn(*entity) {
                    warn!(error = %e, id = %id, "failed to despawn entity");
                }
                false
            }
        });
    }

    #[must_use]
    pub fn transform_of(&self, id: EntityId) -> Option<Transform> {
        let entity = *self.entities.get(&id)?;
        self.world.get::<&Transform>(entity).ok().map(|t| *t)
    }

    pub fn set_transform(&mut self, id: EntityId, transform: Transform) {
        let Some(&entity) = self.entities.get(&id) else {
            return;
        };
        if let Ok(mut t) = self.world.get::<&mut Transform>(entity) {
            *t = transform;
        }
    }

    fn upsert_entity(&mut self, id: EntityId, transform: Transform) {
        if self.entities.contains_key(&id) {
            self.update_entity(id, transform);
        } else {
            self.spawn_entity(id, transform);
        }
    }

    fn spawn_entity(&mut self, id: EntityId, transform: Transform) -> Entity {
        let entity = self.world.spawn((transform,));
        self.entities.insert(id, entity);
        info!(id = %id, transform = ?transform, "entity spawned");
        entity
    }

    fn update_entity(&mut self, id: EntityId, transform: Transform) {
        let Some(&entity) = self.entities.get(&id) else {
            warn!(id = %id, "update requested for unknown entity");
            return;
        };
        if let Ok(mut t) = self.world.get::<&mut Transform>(entity) {
            *t = transform;
            trace!(id = %id, transform = ?transform, "entity transform updated");
        } else if let Err(e) = self.world.insert_one(entity, transform) {
            error!(error = %e, id = %id, "failed to insert transform");
        } else {
            trace!(id = %id, transform = ?transform, "entity transform inserted");
        }
    }
}

/// Merge a partial delta onto an optional current transform. Returns `None`
/// only when a field is absent from both the delta and the current state,
/// which indicates a new entity arriving with an incomplete delta (a bug on
/// the server side).
fn merge_delta(current: Option<Transform>, delta: &EntityDelta) -> Option<Transform> {
    let translation = delta
        .translation
        .map(Into::into)
        .or_else(|| current.map(|t| t.translation))?;
    let rotation = delta
        .rotation
        .map(Quat::from_array)
        .or_else(|| current.map(|t| t.rotation))?;
    Some(Transform {
        translation,
        rotation,
    })
}
