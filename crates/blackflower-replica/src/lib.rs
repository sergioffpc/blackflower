use std::{
    collections::VecDeque,
    net::SocketAddr,
    sync::{
        Arc,
        atomic::{AtomicBool, Ordering},
    },
    thread::JoinHandle,
    time::{Duration, Instant},
};

use anyhow::Context;
use arc_swap::ArcSwap;
use blackflower_entity::EntityId;
use blackflower_gameplay::systems::apply_player_movement;
use blackflower_input::{InputHandle, components::InputButtons};
use blackflower_math::components::Transform;
use blackflower_network::{
    client::{self, ClientHandle},
    delay::DelayConfig,
};
use blackflower_protocol::{Command, Event, Request, Snapshot};
use blackflower_tick::{Tick, TickScheduler};
use blackflower_world::presentation::{
    EntityState, PresentationState, PresentationWorld, interpolate,
};
use tracing::{debug, error};

type PresentationBuffer = Arc<ArcSwap<PresentationState>>;

const INTERP_DELAY_TICKS: f64 = 2.0;

pub type ReplicaState = Box<[Transform]>;

#[derive(Clone, Copy, Debug, Default)]
pub struct ReplicaConfig {
    pub latency_ms: u64,
    pub jitter_ms: u64,
}

pub struct Replica {
    tick_hz: u64,
    clock_anchor_tick: Tick,
    clock_anchor_instant: Instant,

    assigned_entity_id: EntityId,
    presentation_buffer: PresentationBuffer,

    input_handle: Arc<InputHandle>,
    network_handle: Arc<ClientHandle<Command, Snapshot, Request, Event>>,

    tick_stop: Arc<AtomicBool>,
    tick_handle: Option<JoinHandle<()>>,
}

impl Drop for Replica {
    fn drop(&mut self) {
        // Signal the tick thread to exit after its current tick completes.
        self.tick_stop.store(true, Ordering::Relaxed);
        if let Some(handle) = self.tick_handle.take() {
            handle.join().ok();
        }
        // After the tick thread exits it drops its Arc<ClientHandle>, bringing
        // the refcount to 0.  ClientHandle::drop then signals the network
        // thread, which calls connection.close() before exiting.
    }
}

impl Replica {
    pub fn connect(addr: SocketAddr, config: ReplicaConfig) -> anyhow::Result<Self> {
        let input_handle = Arc::new(InputHandle::default());

        let delay = DelayConfig::from_millis(config.latency_ms, config.jitter_ms);
        let network_handle =
            Arc::new(client::connect(addr, delay).context("connecting to server")?);

        network_handle.try_send_request(Request::Hello);
        let Event::Welcome {
            tick_hz,
            assigned_entity_id,
        } = loop {
            if let Some(welcome @ Event::Welcome { .. }) = network_handle.try_recv_events().next() {
                break welcome;
            }

            std::thread::sleep(Duration::from_millis(5));
        };

        let presentation_buffer: PresentationBuffer =
            Arc::new(ArcSwap::from_pointee(PresentationState::default()));

        Ok(Self {
            tick_hz,
            clock_anchor_tick: Tick::ZERO,
            clock_anchor_instant: Instant::now(),

            assigned_entity_id: EntityId::from(assigned_entity_id),
            presentation_buffer,

            input_handle,
            network_handle,

            tick_stop: Arc::new(AtomicBool::new(false)),
            tick_handle: None,
        })
    }

    pub fn start(&mut self) -> anyhow::Result<()> {
        let tick_presentation_buffer = self.presentation_buffer.clone();
        let tick_network_handle = self.network_handle.clone();
        let tick_input_handle = self.input_handle.clone();
        let tick_hz = self.tick_hz;

        let scheduler = TickScheduler::new(tick_hz);
        self.tick_stop = scheduler.stop_handle();

        let mut world = PresentationWorld::default();
        let mut prediction = PredictionState::new(self.assigned_entity_id);

        let handle = std::thread::Builder::new()
        .name("blackflower-runtime::tick".to_owned())
        .spawn(move || {
            let dt = 1.0 / tick_hz as f32;

            let result = scheduler.start(|tick, _dt| {
                let mut latest_ack: Option<Tick> = None;
                tick_network_handle.try_recv_snapshots().for_each(|snapshot| {
                    world.apply(&snapshot.world, Tick::from(snapshot.tick));

                    let ack = Tick::from(snapshot.ack);
                    latest_ack = Some(latest_ack.map_or(ack, |cur| cur.max(ack)));
                });

                let command = tick_input_handle.command(tick);
                if tick.as_u64() % tick_hz == 0 {
                    debug!(tick = %tick, buttons = ?InputButtons::from_bits(command.buttons).unwrap_or_default(), "input command");
                }

                #[allow(clippy::excessive_nesting)]
                if let Some(local) = prediction.local_player() {
                    if let (Some(ack), Some(authoritative)) =
                        (latest_ack, world.transform_of(local))
                    {
                        prediction.reconcile(authoritative, ack, dt);
                    }

                    let buttons = InputButtons::from_bits(command.buttons).unwrap_or_default();
                    let seed = world.transform_of(local);
                    if let Some(predicted) = prediction.predict(tick, buttons, seed, dt) {
                        world.set_transform(local, predicted);
                    }
                }

                tick_network_handle.try_send_command(command);

                let local = prediction
                    .local_player()
                    .zip(prediction.local_transform());
                let state = Arc::new(world.extract(local));
                if tick.as_u64() % tick_hz == 0 {
                    debug!(tick = %tick, state = ?state, "publish render state");
                }
                tick_presentation_buffer.store(state);
            });
            if let Err(e) = result {
                error!(error = %e, "tick thread terminated");
            }
        }).context("spawning tick thread")?;

        self.tick_handle = Some(handle);
        Ok(())
    }

    pub fn clear_buttons(&self) {
        self.input_handle.clear();
    }

    pub fn press_button(&self, button: InputButtons) {
        self.input_handle.press(button);
    }

    pub fn release_button(&self, button: InputButtons) {
        self.input_handle.release(button);
    }

    pub fn state(&mut self, now: Instant) -> ReplicaState {
        let state = self.presentation_buffer.load_full();
        self.resolve(&state, now).into_boxed_slice()
    }

    fn server_time_now(&mut self, state: &PresentationState, now: Instant) -> f64 {
        if state.latest_tick > self.clock_anchor_tick {
            self.clock_anchor_tick = state.latest_tick;
            self.clock_anchor_instant = now;
        }
        let elapsed = now.duration_since(self.clock_anchor_instant).as_secs_f64();
        elapsed.mul_add(self.tick_hz as f64, self.clock_anchor_tick.as_f64())
    }

    fn resolve(&mut self, state: &PresentationState, now: Instant) -> Vec<Transform> {
        let target = self.server_time_now(state, now) - INTERP_DELAY_TICKS;
        state
            .entities
            .iter()
            .filter_map(|(_, render)| match render {
                EntityState::Predicted(t) => Some(*t),
                EntityState::Interpolated(samples) => interpolate(samples, target),
            })
            .collect()
    }
}

const HISTORY_CAPACITY: usize = 128;

#[derive(Clone, Copy, Debug)]
struct HistoryEntry {
    tick: Tick,
    buttons: InputButtons,
    predicted: Transform,
}

#[derive(Debug, Default)]
struct PredictionState {
    local_player: Option<EntityId>,
    local_transform: Option<Transform>,
    history: VecDeque<HistoryEntry>,
}

impl PredictionState {
    #[must_use]
    fn new(entity: EntityId) -> Self {
        Self {
            local_player: Some(entity),
            local_transform: None,
            history: VecDeque::with_capacity(HISTORY_CAPACITY),
        }
    }

    #[must_use]
    const fn local_player(&self) -> Option<EntityId> {
        self.local_player
    }

    #[must_use]
    const fn local_transform(&self) -> Option<Transform> {
        self.local_transform
    }

    fn predict(
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

    fn reconcile(&mut self, authoritative: Transform, last_acked: Tick, dt: f32) {
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
