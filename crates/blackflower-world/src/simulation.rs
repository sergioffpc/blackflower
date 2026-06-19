use blackflower_math::components::Transform;
use blackflower_protocol::{
    EntityDelta, EntitySnapshot, Properties, Property, PropertyDelta, WorldDelta, WorldSnapshot,
};
use blackflower_time::Tick;
use hashbrown::HashMap;
use hecs::DynamicBundle;

use crate::{Entities, EntityId};

#[derive(Default)]
pub struct SimulationWorld {
    entities: Entities,
}

impl SimulationWorld {
    pub fn query<Q: hecs::Query>(&self) -> hecs::QueryBorrow<'_, Q> {
        self.entities.query::<Q>()
    }

    pub fn query_mut<Q: hecs::Query>(&mut self) -> hecs::QueryMut<'_, Q> {
        self.entities.query_mut::<Q>()
    }

    pub fn spawn(&mut self, components: impl DynamicBundle) -> EntityId {
        self.entities.spawn(components)
    }

    pub fn despawn(&mut self, id: EntityId) {
        self.entities.despawn(id);
    }

    pub fn transform_mut(
        &mut self,
        id: EntityId,
    ) -> Result<hecs::RefMut<'_, Transform>, hecs::ComponentError> {
        self.entities.transform_mut(id)
    }

    pub fn props_mut(
        &mut self,
        id: EntityId,
    ) -> Result<hecs::RefMut<'_, Properties>, hecs::ComponentError> {
        self.entities.props_mut(id)
    }

    /// Hitscan-targetable entities — those carrying both a `Transform` and
    /// `Properties` (i.e. players). Returns `(id, transform)` pairs.
    #[must_use]
    pub fn targets(&self) -> Vec<(EntityId, Transform)> {
        self.entities
            .iter()
            .filter_map(|(&id, &entity)| {
                let transform = self.entities.get::<&Transform>(entity).ok()?;
                self.entities.get::<&Properties>(entity).ok()?;
                Some((id, *transform))
            })
            .collect()
    }

    pub fn full_snapshot(&self) -> WorldSnapshot {
        let entities = self
            .entities
            .iter()
            .filter_map(|(&id, &entity)| {
                let transform = self.entities.get::<&Transform>(entity).ok()?;
                let props = self
                    .entities
                    .get::<&Properties>(entity)
                    .map(|p| p.to_vec())
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
fn diff_props(curr: &[Property], base: &[Property]) -> (Properties, Vec<u16>) {
    let mut changed = Vec::new();
    let mut removed = Vec::new();

    for base_prop in base {
        if !curr.iter().any(|p| p.0 == base_prop.0) {
            removed.push(base_prop.0);
        }
    }
    for curr_prop in curr {
        let baseline_val = base.iter().find(|p| p.0 == curr_prop.0).map(|p| &p.1);
        let is_new_or_changed = baseline_val.is_none_or(|v| *v != curr_prop.1);
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
