use std::collections::VecDeque;

use blackflower_entity::EntityId;
use blackflower_math::{Quat, components::Transform};
use blackflower_protocol::{EntityDelta, WorldDelta};
use blackflower_tick::Tick;
use hashbrown::HashMap;
use hecs::{Entity, World};
use tracing::{error, info, trace, warn};

#[derive(Default)]
pub struct PresentationWorld {
    world: World,
    entities: HashMap<EntityId, Entity>,
    history: HashMap<EntityId, VecDeque<TransformSample>>,
    last_applied: Option<Tick>,
}

impl PresentationWorld {
    /// Apply a snapshot (full or delta) to the presentation world.
    /// Full snapshots (`baseline == 0`) reconcile the full entity set;
    /// delta snapshots only touch entities listed in `removed`/`entities`.
    pub fn apply_delta(&mut self, snapshot: &WorldDelta, tick: Tick) {
        if let Some(last) = self.last_applied
            && tick <= last
        {
            trace!(tick = %tick, last = %last, "dropping stale snapshot");
            return;
        }
        self.last_applied = Some(tick);

        if snapshot.baseline == 0 {
            self.apply_full(snapshot, tick);
        } else {
            self.apply_incremental(snapshot, tick);
        }
    }

    fn apply_full(&mut self, snapshot: &WorldDelta, tick: Tick) {
        let present: hashbrown::HashSet<EntityId> = snapshot
            .entities
            .iter()
            .map(|d| EntityId::from(d.id))
            .collect();

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
        self.history.retain(|id, _| present.contains(id));

        for delta in &snapshot.entities {
            let id = EntityId::from(delta.id);
            if let Some(transform) = merge_delta(None, delta) {
                self.upsert_entity(id, transform);
                self.push_sample(id, tick, transform);
            }
        }
    }

    fn apply_incremental(&mut self, snapshot: &WorldDelta, tick: Tick) {
        for &id_raw in &snapshot.removed {
            let id = EntityId::from(id_raw);
            if let Some(entity) = self.entities.remove(&id)
                && let Err(e) = self.world.despawn(entity)
            {
                warn!(error = %e, id = %id, "failed to despawn entity");
            }
            self.history.remove(&id);
        }

        for delta in &snapshot.entities {
            let id = EntityId::from(delta.id);
            let current = self.transform_of(id);
            if let Some(transform) = merge_delta(current, delta) {
                self.upsert_entity(id, transform);
                self.push_sample(id, tick, transform);
            } else {
                warn!(id = %id, "delta has no transform for unknown entity — skipped");
            }
        }
    }

    fn push_sample(&mut self, id: EntityId, tick: Tick, transform: Transform) {
        let buf = self.history.entry(id).or_default();
        debug_assert!(
            buf.back().is_none_or(|s| s.tick < tick),
            "push_sample called with non-monotonic tick; apply must reject stale snapshots first"
        );
        if buf.len() == INTERP_HISTORY {
            buf.pop_front();
        }
        buf.push_back(TransformSample { tick, transform });
    }

    #[must_use]
    pub fn extract(&self, local: Option<(EntityId, Transform)>) -> PresentationState {
        let latest_tick = self
            .history
            .values()
            .filter_map(|buf| buf.back().map(|s| s.tick))
            .max()
            .unwrap_or(Tick::ZERO);

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
        let samples = self
            .history
            .get(&id)
            .map(|buf| buf.iter().copied().collect::<Box<[TransformSample]>>())
            .unwrap_or_default();
        EntityState::Interpolated(samples)
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

pub const INTERP_HISTORY: usize = 8;

#[derive(Clone, Copy, Debug)]
pub struct TransformSample {
    pub tick: Tick,
    pub transform: Transform,
}

#[derive(Clone, Debug)]
pub enum EntityState {
    Predicted(Transform),
    Interpolated(Box<[TransformSample]>),
}

#[derive(Clone, Debug, Default)]
pub struct PresentationState {
    pub latest_tick: Tick,
    pub entities: Box<[(EntityId, EntityState)]>,
}

#[must_use]
pub fn interpolate(samples: &[TransformSample], target: f64) -> Option<Transform> {
    match samples {
        [] => None,
        [only] => Some(only.transform),
        _ => interpolate_bracketed(samples, target),
    }
}

fn interpolate_bracketed(samples: &[TransformSample], target: f64) -> Option<Transform> {
    let newest = samples[samples.len() - 1];
    let oldest = samples[0];
    if target >= newest.tick.as_f64() {
        return Some(newest.transform);
    }
    if target <= oldest.tick.as_f64() {
        return Some(oldest.transform);
    }
    samples.windows(2).find_map(|w| {
        let a = w[0];
        let b = w[1];
        let (lo, hi) = (a.tick.as_f64(), b.tick.as_f64());
        if target >= lo && target < hi {
            let span = hi - lo;
            let t = if span > 0.0 {
                ((target - lo) / span) as f32
            } else {
                0.0
            };
            Some(a.transform.lerp(b.transform, t))
        } else {
            None
        }
    })
}
