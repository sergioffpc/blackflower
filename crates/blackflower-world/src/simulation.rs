use blackflower_entity::{EntityId, EntityIdAllocator};
use blackflower_math::components::Transform;
use blackflower_protocol::{EntitySnapshot, Snapshot};
use blackflower_tick::Tick;
use hashbrown::HashMap;
use hecs::{DynamicBundle, Entity, World};

#[derive(Default)]
pub struct SimulationWorld {
    entities: World,
    entity_id_allocator: EntityIdAllocator,
    entity_lookup: HashMap<EntityId, Entity>,
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
        self.entity_lookup.insert(entity_id, entity);
        entity_id
    }

    /// Despawn an entity by id, removing it from the world and the lookup.
    /// A no-op if the id is unknown.
    pub fn despawn(&mut self, id: EntityId) {
        if let Some(entity) = self.entity_lookup.remove(&id) {
            self.entities.despawn(entity).ok();
        }
    }

    /// Mutable access to a specific entity's transform, by id.
    pub fn transform_mut(
        &mut self,
        id: EntityId,
    ) -> Result<hecs::RefMut<'_, Transform>, hecs::ComponentError> {
        let entity = self
            .entity_lookup
            .get(&id)
            .copied()
            .ok_or(hecs::ComponentError::NoSuchEntity)?;
        self.entities.get::<&mut Transform>(entity)
    }

    pub fn snapshot(&self, tick: Tick, ack: Tick) -> Snapshot {
        let entities = self
            .entity_lookup
            .iter()
            .filter_map(|(&id, &entity)| {
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
            tick: tick.as_u64(),
            ack: ack.as_u64(),
            entities,
        }
    }
}
