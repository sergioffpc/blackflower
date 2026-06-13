use std::collections::VecDeque;

use blackflower_entity::EntityId;
use blackflower_math::{Quat, components::Transform};
use blackflower_protocol::Snapshot;
use blackflower_tick::Tick;
use hashbrown::HashMap;
use hecs::{Entity, World};
use tracing::{error, info, trace, warn};

#[derive(Default)]
pub struct PresentationWorld {
    entities: World,
    entity_lookup: HashMap<EntityId, Entity>,
    /// Per-entity authoritative sample history for remote interpolation,
    /// newest at the back. The local player is also recorded here but the
    /// render path ignores its history in favor of the predicted transform.
    history: HashMap<EntityId, VecDeque<Sample>>,
}

impl PresentationWorld {
    pub fn apply(&mut self, snapshot: &Snapshot) {
        let server_tick = Tick::from(snapshot.tick);
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
        // Drop history for entities that left.
        self.history.retain(|id, _| present.contains(id));

        // Upsert remaining entities and record an interpolation sample.
        for entity in &snapshot.entities {
            let id = EntityId::from(entity.id);
            let transform = Transform {
                translation: entity.translation.into(),
                rotation: Quat::from_array(entity.rotation),
            };
            self.upsert_entity(id, transform);
            self.push_sample(id, server_tick, transform);
        }
    }

    /// Append an authoritative sample, keeping the per-entity buffer
    /// monotonic in `server_tick`. Out-of-order or duplicate ticks (which
    /// the jittered datagram path can produce) are dropped so the buffer
    /// stays sorted for interpolation's bracket search.
    fn push_sample(&mut self, id: EntityId, server_tick: Tick, transform: Transform) {
        let buf = self.history.entry(id).or_default();
        if buf.back().is_some_and(|s| server_tick <= s.server_tick) {
            return;
        }
        if buf.len() == INTERP_HISTORY {
            buf.pop_front();
        }
        buf.push_back(Sample {
            server_tick,
            transform,
        });
    }

    /// Build the immutable render payload.
    ///
    /// `local` is the client's own avatar and its *predicted* transform, if
    /// prediction is active and seeded. That entity is emitted as
    /// [`RenderEntity::Predicted`] and drawn as-is; every other entity is
    /// emitted with its sample history for the render thread to interpolate.
    #[must_use]
    pub fn extract(&self, local: Option<(EntityId, Transform)>) -> RenderState {
        let latest_server_tick = self
            .history
            .values()
            .filter_map(|buf| buf.back().map(|s| s.server_tick))
            .max()
            .unwrap_or(Tick::ZERO);

        let entities = self
            .entity_lookup
            .keys()
            .map(|&id| (id, self.classify(id, local)))
            .collect();

        RenderState {
            latest_server_tick,
            entities,
        }
    }

    fn classify(&self, id: EntityId, local: Option<(EntityId, Transform)>) -> RenderEntity {
        if let Some((local_id, predicted)) = local
            && local_id == id
        {
            return RenderEntity::Predicted(predicted);
        }
        let samples = self
            .history
            .get(&id)
            .map(|buf| buf.iter().copied().collect::<Box<[Sample]>>())
            .unwrap_or_default();
        RenderEntity::Interpolated(samples)
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

/// Number of authoritative samples retained per entity for interpolation.
/// At 60 Hz this is ~133 ms of history — comfortably more than the 50 ms
/// interpolation delay plus jitter headroom.
pub const INTERP_HISTORY: usize = 8;

/// A single authoritative sample: the server tick it came from and the
/// transform at that tick. Render-thread interpolation picks the two
/// samples bracketing its target time and lerps between them.
#[derive(Clone, Copy, Debug)]
pub struct Sample {
    pub server_tick: Tick,
    pub transform: Transform,
}

/// What the render thread needs to draw one entity.
#[derive(Clone, Debug)]
pub enum RenderEntity {
    /// The local player: already predicted, draw as-is, no interpolation.
    Predicted(Transform),
    /// A remote: interpolate across these samples (newest at back) against
    /// the render clock.
    Interpolated(Box<[Sample]>),
}

/// Immutable per-frame payload published to the render thread.
///
/// Carries enough authoritative history for the render thread to do its
/// own interpolation against its own clock, without sharing the (tick-
/// thread-owned) `PresentationWorld`.
#[derive(Clone, Debug, Default)]
pub struct RenderState {
    /// Newest server tick observed across all entities. The render thread
    /// anchors its server-time estimate to this.
    pub latest_server_tick: Tick,
    pub entities: Box<[(EntityId, RenderEntity)]>,
}

/// Interpolate a remote's transform at a fractional server tick.
///
/// `samples` are monotonic in `server_tick`, newest at the back. `target`
/// is the server time (in fractional ticks) the render wants to display —
/// typically the estimated server tick now, minus the interpolation delay.
///
/// Picks the two samples bracketing `target` and lerps. Clamps to the ends
/// when `target` falls outside the buffer; M3 never extrapolates.
#[must_use]
pub fn interpolate(samples: &[Sample], target: f64) -> Option<Transform> {
    match samples {
        [] => None,
        [only] => Some(only.transform),
        _ => interpolate_bracketed(samples, target),
    }
}

fn interpolate_bracketed(samples: &[Sample], target: f64) -> Option<Transform> {
    let newest = samples[samples.len() - 1];
    let oldest = samples[0];
    if target >= newest.server_tick.as_f64() {
        return Some(newest.transform);
    }
    if target <= oldest.server_tick.as_f64() {
        return Some(oldest.transform);
    }
    samples.windows(2).find_map(|w| {
        let a = w[0];
        let b = w[1];
        let (lo, hi) = (a.server_tick.as_f64(), b.server_tick.as_f64());
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
