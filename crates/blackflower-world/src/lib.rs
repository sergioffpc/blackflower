use blackflower_entity::{EntityId, EntityIdAllocator};
use blackflower_math::{Quat, components::Transform};
use blackflower_protocol::{EntitySnapshot, Snapshot};
use blackflower_tick::Tick;
use hashbrown::HashMap;
use hecs::{DynamicBundle, Entity, World};
use tracing::{error, info, trace, warn};

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
                        id: id.into(),
                        translation: transform.translation.into(),
                        rotation: transform.rotation.into(),
                    })
            })
            .collect();

        Snapshot {
            tick: tick.into(),
            entities,
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
        let present: hashbrown::HashSet<EntityId> =
            snapshot.entities.iter().map(|e| e.id.into()).collect();

        // Despawn entities no longer present in the snapshot.
        self.entity_lookup.retain(|id, entity| {
            if present.contains(id) {
                true
            } else {
                #[allow(clippy::excessive_nesting)]
                if let Err(e) = self.entities.despawn(*entity) {
                    warn!(error = %e, id = %id, "failed to despawn entity");
                }
                false
            }
        });

        // Upsert remaining entities.
        for entity in &snapshot.entities {
            let transform = Transform {
                translation: entity.translation.into(),
                rotation: Quat::from_array(entity.rotation),
            };
            self.upsert_entity(entity.id.into(), transform);
        }
    }

    /// Extract a flat, render-ready snapshot of all entities.
    ///
    /// Produces an owned slice of `(EntityId, Transform)` decoupled from
    /// the ECS, suitable for publishing to the render thread. Order is
    /// unspecified (hecs iteration order); consumers must key by
    /// `EntityId`, not by position.
    #[must_use]
    pub fn extract(&self) -> Box<[(EntityId, Transform)]> {
        self.entity_lookup
            .iter()
            .filter_map(|(&id, &entity)| {
                self.entities
                    .get::<&Transform>(entity)
                    .ok()
                    .map(|t| (id, *t))
            })
            .collect()
    }

    /// Read the transform of a specific entity, if present.
    #[must_use]
    pub fn transform_of(&self, id: EntityId) -> Option<Transform> {
        let entity = *self.entity_lookup.get(&id)?;
        self.entities.get::<&Transform>(entity).ok().map(|t| *t)
    }

    /// Overwrite the transform of a specific entity, if present.
    ///
    /// Used by the prediction layer to replace the local player's
    /// authoritative pose with the locally-predicted one before
    /// extraction. A no-op if the entity is unknown.
    pub fn set_transform(&mut self, id: EntityId, transform: Transform) {
        let Some(&entity) = self.entity_lookup.get(&id) else {
            return;
        };
        if let Ok(mut t) = self.entities.get::<&mut Transform>(entity) {
            *t = transform;
        }
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
