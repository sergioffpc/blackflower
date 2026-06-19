use blackflower_math::components::Transform;
use blackflower_protocol::WorldDelta;
use blackflower_time::Tick;
use tracing::trace;

use crate::{
    Entities, EntityId,
    replication::{InterpolationSample, InterpolationTrack, Replication},
};

#[derive(Default)]
pub struct PresentationWorld {
    entities: Entities,

    last_applied: Option<Tick>,
    replication: Replication,
}

impl PresentationWorld {
    /// Apply a snapshot (full or delta) to the presentation world.
    /// Full snapshots (`baseline == 0`) reconcile the full entity set;
    /// delta snapshots only touch entities listed in `removed`/`entities`.
    pub fn merge(&mut self, snapshot: &WorldDelta, tick: Tick) {
        if let Some(last) = self.last_applied
            && tick <= last
        {
            trace!(tick = %tick, last = %last, "dropping stale snapshot");
            return;
        }
        self.last_applied = Some(tick);

        if snapshot.baseline == 0 {
            let present: hashbrown::HashSet<EntityId> = snapshot
                .entities
                .iter()
                .map(|d| EntityId::from(d.id))
                .collect();
            self.entities.retain_present_entities(&present);
            self.replication.retain_present_entities(&present);

            for delta in &snapshot.entities {
                let id = EntityId::from(delta.id);
                if let Some(transform) = self.entities.apply(id, delta) {
                    self.replication
                        .push_sample(id, InterpolationSample { tick, transform });
                }
            }
        } else {
            for &id_raw in &snapshot.removed {
                let id = EntityId::from(id_raw);
                self.entities.remove(&id);
                self.replication.remove(&id);
            }

            for delta in &snapshot.entities {
                let id = EntityId::from(delta.id);
                if let Some(transform) = self.entities.merge(id, delta) {
                    self.replication
                        .push_sample(id, InterpolationSample { tick, transform });
                }
            }
        }
    }

    #[must_use]
    pub fn state(&self, local: Option<(EntityId, Transform)>) -> PresentationState {
        let latest_tick = self.replication.latest_tick();
        let entities = self
            .entities
            .keys()
            .map(|&id| (id, self.classify(id, local)))
            .collect();

        PresentationState {
            latest_tick,
            entities,
        }
    }

    fn classify(&self, id: EntityId, local: Option<(EntityId, Transform)>) -> EntityState {
        if let Some((local_id, predicted)) = local
            && local_id == id
        {
            return EntityState::Predicted(predicted);
        }
        let samples = self.replication.interpolation_track(id);
        EntityState::Interpolated(samples)
    }

    #[must_use]
    pub fn transform_of(&self, id: EntityId) -> Option<Transform> {
        self.entities.transform_of(id)
    }

    pub fn set_transform(&mut self, id: EntityId, transform: Transform) {
        self.entities.set_transform(id, transform);
    }
}

#[derive(Clone, Debug)]
pub enum EntityState {
    Predicted(Transform),
    Interpolated(InterpolationTrack),
}

#[derive(Clone, Debug, Default)]
pub struct PresentationState {
    pub latest_tick: Tick,
    pub entities: Box<[(EntityId, EntityState)]>,
}
