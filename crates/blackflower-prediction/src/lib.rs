//! Client-side prediction state.
//!
//! Holds the locally-predicted transform of the player's own avatar,
//! plus a ring buffer of recent predictions for later reconciliation
//! (M2.5). This crate is an optional layer: removing it from the client
//! pipeline leaves the world showing only authoritative server state.
//!
//! Prediction reuses [`blackflower_gameplay::systems::apply_player_movement`]
//! — the exact same function the server runs — so that a correctly
//! predicted input produces a transform identical to the server's,
//! provided the caller passes the same `dt` the server uses.

use std::collections::VecDeque;

use blackflower_entity::EntityId;
use blackflower_gameplay::systems::apply_player_movement;
use blackflower_input::components::InputButtons;
use blackflower_math::components::Transform;
use blackflower_tick::Tick;

/// Maximum number of predicted ticks retained for reconciliation.
///
/// At 60 Hz this is ~2.1 s of history — far more than any plausible
/// RTT. Older entries are evicted as new ones arrive.
const HISTORY_CAPACITY: usize = 128;

/// A single predicted tick: the input applied and the transform that
/// resulted from applying it. Retained so that M2.5 reconciliation can
/// replay inputs after an authoritative correction.
#[derive(Clone, Copy, Debug)]
pub struct HistoryEntry {
    pub tick: Tick,
    pub buttons: InputButtons,
    pub predicted: Transform,
}

/// Local prediction for the player's own avatar.
///
/// The state is meaningless until [`assign`](Self::assign) names the
/// local entity and the first [`predict`](Self::predict) seeds the
/// transform from the authoritative world.
#[derive(Debug, Default)]
pub struct PredictionState {
    /// The entity this client controls. `None` until the server's
    /// `Welcome` is processed.
    local_player: Option<EntityId>,
    /// Whether `local_transform` has been seeded from the world yet.
    seeded: bool,
    /// The locally-predicted transform of the local player.
    local_transform: Transform,
    /// Ring buffer of recent predictions, oldest at the front.
    history: VecDeque<HistoryEntry>,
}

impl PredictionState {
    #[must_use]
    pub fn new() -> Self {
        Self {
            local_player: None,
            seeded: false,
            local_transform: Transform::identity(),
            history: VecDeque::with_capacity(HISTORY_CAPACITY),
        }
    }

    /// Name the entity this client controls. Called when the server's
    /// `Welcome` is processed. Idempotent re-assignment to the same id
    /// is a no-op; a *different* id resets the prediction.
    pub fn assign(&mut self, entity: EntityId) {
        if self.local_player == Some(entity) {
            return;
        }
        self.local_player = Some(entity);
        self.seeded = false;
        self.history.clear();
    }

    /// The entity this client controls, if assigned.
    #[must_use]
    pub const fn local_player(&self) -> Option<EntityId> {
        self.local_player
    }

    /// The current predicted transform, if prediction is active and
    /// seeded.
    #[must_use]
    pub const fn local_transform(&self) -> Option<Transform> {
        if self.local_player.is_some() && self.seeded {
            Some(self.local_transform)
        } else {
            None
        }
    }

    /// Advance the prediction by one tick.
    ///
    /// `seed` is the authoritative transform of the local player as
    /// currently known (e.g. read from the presentation world). On the
    /// first call after assignment it initializes the predicted
    /// transform; afterwards it is ignored and prediction runs forward
    /// from its own state.
    ///
    /// `dt` must match the server's simulation step (`1.0 / tick_hz`) so
    /// that identical inputs produce identical transforms on both ends.
    ///
    /// Returns the newly predicted transform if prediction is active and
    /// seeded, or `None` otherwise — in which case the caller should
    /// leave the authoritative state untouched.
    pub fn predict(
        &mut self,
        tick: Tick,
        buttons: InputButtons,
        seed: Option<Transform>,
        dt: f32,
    ) -> Option<Transform> {
        self.local_player?;

        if !self.seeded {
            // Seed lazily from the authoritative world. If the world
            // does not yet contain our entity, stay unseeded and wait.
            let seed = seed?;
            self.local_transform = seed;
            self.seeded = true;
        }

        apply_player_movement(&mut self.local_transform, buttons, dt);

        self.push_history(HistoryEntry {
            tick,
            buttons,
            predicted: self.local_transform,
        });

        Some(self.local_transform)
    }

    /// Reconcile the local prediction against an authoritative snapshot.
    ///
    /// Called when a snapshot arrives carrying the local player's
    /// authoritative transform and `last_acked` — the client tick of the
    /// last command the server has processed for us. Performs the
    /// rollback-and-replay that makes prediction correct rather than
    /// merely optimistic:
    ///
    /// 1. Drop history entries the server has already accounted for
    ///    (`tick <= last_acked`); their inputs are baked into
    ///    `authoritative`.
    /// 2. Roll back the predicted transform to `authoritative`.
    /// 3. Replay every remaining (unacked) input on top, in order,
    ///    using the same `dt` the server uses.
    ///
    /// A no-op if prediction is not yet assigned and seeded — there is
    /// nothing to correct before the first prediction has run.
    pub fn reconcile(&mut self, authoritative: Transform, last_acked: Tick, dt: f32) {
        if self.local_player.is_none() || !self.seeded {
            return;
        }

        // 1. Discard inputs the server has already processed.
        while self
            .history
            .front()
            .is_some_and(|entry| entry.tick <= last_acked)
        {
            self.history.pop_front();
        }

        // 2. Roll back to the server's truth.
        self.local_transform = authoritative;

        // 3. Replay the unacked inputs on top, rewriting their stored
        //    predictions so a subsequent reconcile replays from coherent
        //    state.
        for entry in &mut self.history {
            apply_player_movement(&mut self.local_transform, entry.buttons, dt);
            entry.predicted = self.local_transform;
        }
    }

    fn push_history(&mut self, entry: HistoryEntry) {
        if self.history.len() == HISTORY_CAPACITY {
            self.history.pop_front();
        }
        self.history.push_back(entry);
    }
}
