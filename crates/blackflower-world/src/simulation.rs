use blackflower_entity::{EntityId, EntityIdAllocator};
use blackflower_math::components::Transform;
use blackflower_protocol::{EntitySnapshot, WorldSnapshot};
use hashbrown::HashMap;
use hecs::{DynamicBundle, Entity, World};

#[derive(Default)]
pub struct SimulationWorld {
    world: World,
    allocator: EntityIdAllocator,
    entities: HashMap<EntityId, Entity>,
}

impl SimulationWorld {
    pub fn query<Q: hecs::Query>(&self) -> hecs::QueryBorrow<'_, Q> {
        self.world.query::<Q>()
    }

    pub fn query_mut<Q: hecs::Query>(&mut self) -> hecs::QueryMut<'_, Q> {
        self.world.query_mut::<Q>()
    }

    pub fn spawn(&mut self, components: impl DynamicBundle) -> EntityId {
        let entity = self.world.spawn(components);
        let entity_id = self.allocator.allocate();
        self.entities.insert(entity_id, entity);
        entity_id
    }

    pub fn despawn(&mut self, id: EntityId) {
        if let Some(entity) = self.entities.remove(&id) {
            self.world.despawn(entity).ok();
        }
    }

    pub fn transform_mut(
        &mut self,
        id: EntityId,
    ) -> Result<hecs::RefMut<'_, Transform>, hecs::ComponentError> {
        let entity = self
            .entities
            .get(&id)
            .copied()
            .ok_or(hecs::ComponentError::NoSuchEntity)?;
        self.world.get::<&mut Transform>(entity)
    }

    pub fn snapshot(&self) -> WorldSnapshot {
        let entities = self
            .entities
            .iter()
            .filter_map(|(&id, &entity)| {
                self.world
                    .get::<&Transform>(entity)
                    .ok()
                    .map(|transform| EntitySnapshot {
                        id: id.into(),
                        translation: transform.translation.into(),
                        rotation: transform.rotation.into(),
                    })
            })
            .collect();

        WorldSnapshot { entities }
    }
}
