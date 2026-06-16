use blackflower_entity::{EntityId, EntityIdAllocator};
use blackflower_math::components::Transform;
use blackflower_protocol::{EntitySnapshot, Prop, WorldSnapshot};
use hashbrown::HashMap;
use hecs::{DynamicBundle, Entity, World};

/// Engine-opaque property bag stored per entity.
/// Encoding of each value is owned by the game plugin — the engine never
/// interprets the bytes.
#[derive(Clone, Default)]
pub struct EntityProps(pub Vec<(u16, Vec<u8>)>);

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
                let transform = self.world.get::<&Transform>(entity).ok()?;
                let props = self
                    .world
                    .get::<&EntityProps>(entity)
                    .map(|p| {
                        p.0.iter()
                            .map(|(pid, val)| Prop { id: *pid, value: val.clone() })
                            .collect()
                    })
                    .unwrap_or_default();
                Some(EntitySnapshot {
                    id: id.into(),
                    translation: transform.translation.into(),
                    rotation: transform.rotation.into(),
                    props,
                })
            })
            .collect();

        WorldSnapshot { entities }
    }
}
