use blackflower_math::components::Transform;
use blackflower_protocol::{
    EntityDelta, EntitySnapshot, Property, PropertyDelta, WorldDelta, WorldSnapshot,
};
use blackflower_time::Tick;
use hashbrown::HashMap;
use hecs::{DynamicBundle, Entity, World};

use crate::{EntityId, EntityIdAllocator};

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

    pub fn props_mut(
        &mut self,
        id: EntityId,
    ) -> Result<hecs::RefMut<'_, EntityProps>, hecs::ComponentError> {
        let entity = self
            .entities
            .get(&id)
            .copied()
            .ok_or(hecs::ComponentError::NoSuchEntity)?;
        self.world.get::<&mut EntityProps>(entity)
    }

    /// Hitscan-targetable entities — those carrying both a `Transform` and
    /// `EntityProps` (i.e. players). Returns `(id, transform)` pairs.
    #[must_use]
    pub fn targets(&self) -> Vec<(EntityId, Transform)> {
        self.entities
            .iter()
            .filter_map(|(&id, &entity)| {
                let transform = self.world.get::<&Transform>(entity).ok()?;
                self.world.get::<&EntityProps>(entity).ok()?;
                Some((id, *transform))
            })
            .collect()
    }

    pub fn full_snapshot(&self) -> WorldSnapshot {
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
                            .map(|(pid, val)| Property {
                                id: *pid,
                                data: val.clone(),
                            })
                            .collect()
                    })
                    .unwrap_or_default();
                Some(EntitySnapshot {
                    id: id.into(),
                    translation: transform.translation.into(),
                    rotation: transform.rotation.into(),
                    properties: props,
                })
            })
            .collect();

        WorldSnapshot { entities }
    }

    /// Build a per-client delta from an already-built `current` snapshot (the
    /// caller builds the full snapshot once per tick for the ring and threads
    /// it in here, rather than each client rebuilding it).
    pub fn delta_snapshot(
        current: &WorldSnapshot,
        baseline: Option<&WorldSnapshot>,
        baseline_tick: Tick,
        server_tick: Tick,
        ack: Tick,
    ) -> WorldDelta {
        let Some(base) = baseline else {
            return WorldDelta {
                tick: server_tick.as_u64(),
                ack: ack.as_u64(),
                baseline: 0,
                removed: Box::default(),
                entities: current.entities.iter().map(entity_full_delta).collect(),
            };
        };

        let base_index: HashMap<u64, &EntitySnapshot> =
            base.entities.iter().map(|e| (e.id, e)).collect();
        let curr_ids: hashbrown::HashSet<u64> = current.entities.iter().map(|e| e.id).collect();

        let removed: Box<[u64]> = base
            .entities
            .iter()
            .map(|e| e.id)
            .filter(|id| !curr_ids.contains(id))
            .collect();

        let entities: Box<[EntityDelta]> = current
            .entities
            .iter()
            .filter_map(|curr| entity_delta(curr, base_index.get(&curr.id).copied()))
            .collect();

        WorldDelta {
            tick: server_tick.as_u64(),
            ack: ack.as_u64(),
            baseline: baseline_tick.as_u64(),
            removed,
            entities,
        }
    }
}

fn entity_full_delta(e: &EntitySnapshot) -> EntityDelta {
    EntityDelta {
        id: e.id,
        translation: Some(e.translation),
        rotation: Some(e.rotation),
        properties: PropertyDelta {
            changed_props: e.properties.clone(),
            removed_props: vec![],
        },
    }
}

fn entity_delta(curr: &EntitySnapshot, base: Option<&EntitySnapshot>) -> Option<EntityDelta> {
    let Some(base) = base else {
        return Some(entity_full_delta(curr));
    };
    let translation =
        field_changed(&curr.translation, &base.translation).then_some(curr.translation);
    let rotation = field_changed(&curr.rotation, &base.rotation).then_some(curr.rotation);
    let (changed_props, removed_props) = diff_props(&curr.properties, &base.properties);
    let has_changes = translation.is_some()
        || rotation.is_some()
        || !changed_props.is_empty()
        || !removed_props.is_empty();
    has_changes.then_some(EntityDelta {
        id: curr.id,
        translation,
        rotation,
        properties: PropertyDelta {
            changed_props,
            removed_props,
        },
    })
}

/// Returns `(changed, removed)` props by comparing current vs baseline.
/// Change detection is byte-exact — the engine does not interpret values.
fn diff_props(curr: &[Property], base: &[Property]) -> (Vec<Property>, Vec<u16>) {
    let mut changed = Vec::new();
    let mut removed = Vec::new();

    for base_prop in base {
        if !curr.iter().any(|p| p.id == base_prop.id) {
            removed.push(base_prop.id);
        }
    }
    for curr_prop in curr {
        let baseline_val = base.iter().find(|p| p.id == curr_prop.id).map(|p| &p.data);
        let is_new_or_changed = baseline_val.is_none_or(|v| *v != curr_prop.data);
        if is_new_or_changed {
            changed.push(curr_prop.clone());
        }
    }

    (changed, removed)
}

/// Bit-exact change detection via `f32::to_bits`.
fn field_changed(a: &[f32], b: &[f32]) -> bool {
    a.iter()
        .zip(b.iter())
        .any(|(x, y)| x.to_bits() != y.to_bits())
}
