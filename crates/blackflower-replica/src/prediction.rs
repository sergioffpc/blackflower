use std::collections::VecDeque;

use blackflower_gameplay::systems::apply_player_movement;
use blackflower_input::components::InputButtons;
use blackflower_math::components::Transform;
use blackflower_time::Tick;
use blackflower_world::EntityId;

const HISTORY_CAPACITY: usize = 128;

#[derive(Clone, Copy, Debug)]
struct HistoryEntry {
    tick: Tick,
    buttons: InputButtons,
    predicted: Transform,
}

#[derive(Debug, Default)]
pub(crate) struct PredictionState {
    local_player: Option<EntityId>,
    local_transform: Option<Transform>,
    history: VecDeque<HistoryEntry>,
}

impl PredictionState {
    #[must_use]
    pub(crate) fn new(entity: EntityId) -> Self {
        Self {
            local_player: Some(entity),
            local_transform: None,
            history: VecDeque::with_capacity(HISTORY_CAPACITY),
        }
    }

    #[must_use]
    pub(crate) const fn local_player(&self) -> Option<EntityId> {
        self.local_player
    }

    #[must_use]
    pub(crate) const fn local_transform(&self) -> Option<Transform> {
        self.local_transform
    }

    pub(crate) fn predict(
        &mut self,
        tick: Tick,
        buttons: InputButtons,
        seed: Option<Transform>,
        dt: f32,
    ) -> Option<Transform> {
        self.local_player?;
        let transform = self.local_transform.get_or_insert(seed?);
        apply_player_movement(transform, buttons, dt);
        let predicted = *transform;
        self.push_history(HistoryEntry {
            tick,
            buttons,
            predicted,
        });
        Some(predicted)
    }

    pub(crate) fn reconcile(&mut self, authoritative: Transform, last_acked: Tick, dt: f32) {
        let Some(transform) = self.local_transform.as_mut() else {
            return;
        };

        while self
            .history
            .front()
            .is_some_and(|entry| entry.tick <= last_acked)
        {
            self.history.pop_front();
        }

        *transform = authoritative;

        for entry in &mut self.history {
            apply_player_movement(transform, entry.buttons, dt);
            entry.predicted = *transform;
        }
    }

    fn push_history(&mut self, entry: HistoryEntry) {
        if self.history.len() == HISTORY_CAPACITY {
            self.history.pop_front();
        }
        self.history.push_back(entry);
    }
}
