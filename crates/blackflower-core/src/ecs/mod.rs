pub mod components;
pub mod systems;

use hashbrown::HashMap;
use hecs::{DynamicBundle, Entity, World};
use serde::{Deserialize, Serialize};
use tracing::{error, info, warn};

use crate::{ecs::components::Transform, time::Tick};

/// Stable identifier for a replicated entity.
///
/// Allocated by the server. Wire-stable: an `EntityId` written today and
/// read tomorrow refers to the same logical entity.
///
/// Internally a `u64`. Value `0` is reserved for "no entity" and is never
/// allocated by [`EntityIdAllocator`].
#[repr(transparent)]
#[derive(Clone, Copy, Debug, Default, PartialEq, Eq, Hash, Serialize, Deserialize)]
pub struct EntityId(u64);

impl EntityId {
    /// Sentinel value used to represent "no entity". Equivalent to
    /// `Option::None` when an explicit sentinel is preferable to nesting.
    pub const NONE: Self = Self(0);

    /// Returns `true` if this id is the [`NONE`](Self::NONE) sentinel.
    #[must_use]
    pub const fn is_none(self) -> bool {
        self.0 == 0
    }
}

impl std::fmt::Display for EntityId {
    fn fmt(&self, f: &mut std::fmt::Formatter<'_>) -> std::fmt::Result {
        write!(f, "{}", self.0)
    }
}

/// Allocator for [`EntityId`].
///
/// Hands out sequential IDs starting at `1`. Not thread-safe by itself —
/// owned by the tick thread and accessed without contention.
#[derive(Debug, Default)]
pub struct EntityIdAllocator {
    next: u64,
}

impl EntityIdAllocator {
    /// Create a new allocator. The first ID returned will be `EntityId(1)`.
    #[must_use]
    pub const fn new() -> Self {
        Self { next: 0 }
    }

    /// Allocate a new unique identifier.
    pub const fn allocate(&mut self) -> EntityId {
        self.next += 1;
        EntityId(self.next)
    }
}

/// A snapshot of the entire simulation state at a specific tick.
#[derive(Clone, Debug, Serialize, Deserialize)]
pub struct Snapshot {
    pub tick: Tick,
    pub entities: Box<[EntitySnapshot]>,
    pub events: Box<[EventSnapshot]>,
}

/// Replicated state of a single entity.
#[derive(Clone, Debug, Serialize, Deserialize)]
pub struct EntitySnapshot {
    pub id: EntityId,
    pub transform: Transform,
}

#[derive(Clone, Debug, Serialize, Deserialize)]
pub enum EventSnapshot {
    Despawn(EntityId),
}

#[derive(Default)]
pub struct SimulationWorld {
    entities: World,
    entity_id_allocator: EntityIdAllocator,
    entity_lookup: HashMap<Entity, EntityId>,
}

impl SimulationWorld {
    pub fn query<Q: hecs::Query>(&self) -> hecs::QueryBorrow<'_, Q> {
        self.entities.query::<Q>()
    }

    pub fn query_mut<Q: hecs::Query>(&mut self) -> hecs::QueryMut<'_, Q> {
        self.entities.query_mut::<Q>()
    }

    pub fn spawn(&mut self, components: impl DynamicBundle) -> EntityId {
        let entity = self.entities.spawn(components);
        let entity_id = self.entity_id_allocator.allocate();
        self.entity_lookup.insert(entity, entity_id);
        entity_id
    }

    pub fn snapshot(&self, tick: Tick) -> Snapshot {
        let entities = self
            .entity_lookup
            .iter()
            .filter_map(|(&entity, &id)| {
                self.entities
                    .get::<&Transform>(entity)
                    .ok()
                    .map(|transform| EntitySnapshot {
                        id,
                        transform: *transform,
                    })
            })
            .collect();
        let events = vec![].into_boxed_slice();

        Snapshot {
            tick,
            entities,
            events,
        }
    }
}

#[derive(Default)]
pub struct PresentationWorld {
    entities: World,
    entity_lookup: HashMap<EntityId, Entity>,
}

impl PresentationWorld {
    pub fn query<Q: hecs::Query>(&self) -> hecs::QueryBorrow<'_, Q> {
        self.entities.query::<Q>()
    }

    pub fn query_mut<Q: hecs::Query>(&mut self) -> hecs::QueryMut<'_, Q> {
        self.entities.query_mut::<Q>()
    }

    pub fn apply(&mut self, snapshot: &Snapshot) {
        for ent in &snapshot.entities {
            self.upsert_entity(ent.id, ent.transform);
        }
        // TODO despawn entities
    }

    fn upsert_entity(&mut self, id: EntityId, transform: Transform) {
        if self.entity_lookup.contains_key(&id) {
            self.update_entity(id, transform);
        } else {
            self.spawn_entity(id, transform);
        }
    }

    fn spawn_entity(&mut self, id: EntityId, transform: Transform) -> Entity {
        let entity = self.entities.spawn((transform,));
        self.entity_lookup.insert(id, entity);
        info!(id = %id, transform = ?transform, "entity spawned");
        entity
    }

    fn despawn_entity(&mut self, id: EntityId) {
        let Some(entity) = self.entity_lookup.remove(&id) else {
            warn!(id = %id, "despawn requested for unknown entity");
            return;
        };
        match self.entities.despawn(entity) {
            Ok(()) => info!(id = %id, "entity despawned"),
            Err(e) => warn!(error = %e, id = %id, "failed to despawn entity"),
        }
    }

    fn update_entity(&mut self, id: EntityId, transform: Transform) {
        let Some(&entity) = self.entity_lookup.get(&id) else {
            warn!(id = %id, "update requested for unknown entity");
            return;
        };
        if let Ok(mut t) = self.entities.get::<&mut Transform>(entity) {
            *t = transform;
            info!(id = %id, transform = ?transform, "entity transform updated");
        } else if let Err(e) = self.entities.insert_one(entity, transform) {
            error!(error = %e, id = %id, "failed to insert transform");
        } else {
            info!(id = %id, transform = ?transform, "entity transform inserted");
        }
    }
}
