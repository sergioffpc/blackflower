pub mod components;
pub mod systems;

use hashbrown::HashMap;
use hecs::{DynamicBundle, Entity, World};
use serde::{Deserialize, Serialize};
use tracing::info;

use crate::{ecs::components::Transform, time::Tick};

/// Stable identifier for a replicated entity.
///
/// Allocated by the server. Wire-stable: an `EntityId` written today and
/// read tomorrow refers to the same logical entity.
///
/// Internally a `u64`. Value `0` is reserved for "no entity" and is never
/// allocated by [`EntityIdAllocator`].
#[derive(Clone, Copy, Debug, Default, PartialEq, Eq, Hash, Serialize, Deserialize)]
#[repr(transparent)]
pub struct EntityId(u64);

impl EntityId {
    /// Sentinel value used to represent "no entity". Equivalent to
    /// `Option::None` when an explicit sentinel is preferable to nesting.
    pub const NONE: Self = Self(0);

    /// Returns `true` if this id is the [`NONE`](Self::NONE) sentinel.
    #[must_use]
    pub const fn is_none(self) -> bool {
        self.0 == 0
    }
}

impl std::fmt::Display for EntityId {
    fn fmt(&self, f: &mut std::fmt::Formatter<'_>) -> std::fmt::Result {
        write!(f, "entity id {}", self.0)
    }
}

/// Allocator for [`EntityId`].
///
/// Hands out sequential IDs starting at `1`. Not thread-safe by itself —
/// owned by the tick thread and accessed without contention.
#[derive(Debug, Default)]
pub struct EntityIdAllocator {
    next: u64,
}

impl EntityIdAllocator {
    /// Create a new allocator. The first ID returned will be `EntityId(1)`.
    #[must_use]
    pub const fn new() -> Self {
        Self { next: 0 }
    }

    /// Allocate a new unique identifier.
    pub const fn allocate(&mut self) -> EntityId {
        self.next += 1;
        EntityId(self.next)
    }
}

/// A snapshot of the entire simulation state at a specific tick.
#[derive(Clone, Debug, Serialize, Deserialize)]
pub struct Snapshot {
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
    entity_id_mapping: HashMap<Entity, EntityId>,
}

impl SimulationWorld {
    pub fn spawn(&mut self, components: impl DynamicBundle) -> EntityId {
        let entity = self.entities.spawn(components);
        let entity_id = self.entity_id_allocator.allocate();
        self.entity_id_mapping.insert(entity, entity_id);
        entity_id
    }

    pub fn snapshot(&self, tick: Tick) -> Snapshot {
        let entities = self
            .entity_id_mapping
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
        Snapshot { tick, entities }
    }
}

#[derive(Default)]
pub struct PresentationWorld {
    entities: World,
    entity_id_mapping: HashMap<EntityId, Entity>,
}

impl PresentationWorld {
    pub fn apply(&mut self, snapshot: &Snapshot) {
        if let Some(first) = snapshot.entities.first() {
            info!(
                tick = %snapshot.tick,
                entities = snapshot.entities.len(),
                id = %first.id,
                x = first.transform.translation.x,
                y = first.transform.translation.y,
                z = first.transform.translation.z,
                "received snapshot"
            );
        }

        for entity_snapshot in &snapshot.entities {
            if let Some(entity) = self.entity_id_mapping.get(&entity_snapshot.id).copied() {
                self.entities
                    .insert(entity, (entity_snapshot.transform,))
                    .ok();
            } else {
                let entity = self.entities.spawn((entity_snapshot.transform,));
                self.entity_id_mapping.insert(entity_snapshot.id, entity);
            }
        }
    }
}
