use blackflower_entity::{EntityId, EntityIdAllocator};
use blackflower_math::components::Transform;
use blackflower_tick::Tick;
use hashbrown::HashMap;
use hecs::{DynamicBundle, Entity, World};
use serde::{Deserialize, Serialize};
use tracing::{error, info, trace, warn};

/// A snapshot of the entire simulation state at a specific tick.
#[derive(Clone, Debug, Serialize, Deserialize)]
pub struct WorldSnapshot {
    pub tick: Tick,
    pub entities: Box<[EntitySnapshot]>,
}

/// Replicated state of a single entity.
#[derive(Clone, Debug, Serialize, Deserialize)]
pub struct EntitySnapshot {
    pub id: EntityId,
    pub transform: Transform,
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

    pub fn snapshot(&self, tick: Tick) -> WorldSnapshot {
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

        WorldSnapshot { tick, entities }
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

    pub fn apply(&mut self, snapshot: &WorldSnapshot) {
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
            trace!(id = %id, transform = ?transform, "entity transform updated");
        } else if let Err(e) = self.entities.insert_one(entity, transform) {
            error!(error = %e, id = %id, "failed to insert transform");
        } else {
            trace!(id = %id, transform = ?transform, "entity transform inserted");
        }
    }
}
