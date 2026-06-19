use std::{
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
use blackflower_input::{InputHandle, components::InputButtons};
use blackflower_math::components::Transform;
use blackflower_network::{
    client::{self, ClientHandle},
    delay::DelayConfig,
};
use blackflower_protocol::{Command, Event, PROTOCOL_VERSION, Request, WorldDelta};
use blackflower_time::{Tick, TickScheduler};
use blackflower_world::{
    EntityId,
    presentation::{EntityState, PresentationState, PresentationWorld},
};
use tracing::{debug, error, warn};

use crate::{
    clock::{ClockEstimate, ClockSync},
    prediction::PredictionState,
};

type PresentationBuffer = Arc<ArcSwap<PresentationState>>;

const INTERP_DELAY_TICKS: f64 = 2.0;

/// Sliding-window ack for received snapshots. Bit `i` set in `bits` means the
/// client received the snapshot at tick `ack_tick - i`. Sent piggybacked on
/// every Command so the server can pick a delta baseline.
struct SnapshotAck {
    ack_tick: u64,
    bits: u32,
}

impl SnapshotAck {
    const fn new() -> Self {
        Self {
            ack_tick: 0,
            bits: 0,
        }
    }

    fn record(&mut self, tick: u64) {
        if tick > self.ack_tick {
            let shift = (tick - self.ack_tick).min(32) as u32;
            // Shift old bits right; when shift >= 32 all old bits fall off.
            self.bits = self.bits.checked_shr(shift).unwrap_or(0);
            self.ack_tick = tick;
        }
        let offset = self.ack_tick.saturating_sub(tick);
        if offset < 32 {
            self.bits |= 1_u32 << offset;
        }
    }
}

/// Render-ready frame state.
///
/// The local player's transform drives the first-person camera (`camera`), and
/// every other entity is a world body to draw (`entities`). The local body is
/// excluded — we render from inside it.
#[derive(Clone, Debug, Default)]
pub struct RenderState {
    pub camera: Option<Transform>,
    pub entities: Box<[Transform]>,
}

#[derive(Clone, Copy, Debug, Default)]
pub struct ReplicaConfig {
    pub latency_ms: u64,
    pub jitter_ms: u64,
}

pub struct Replica {
    tick_hz: u64,
    clock_estimate: Arc<ArcSwap<ClockEstimate>>,

    assigned_entity_id: EntityId,
    presentation_buffer: PresentationBuffer,

    input_handle: Arc<InputHandle>,
    network_handle: Arc<ClientHandle<Command, WorldDelta, Request, Event>>,

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

        network_handle.try_send_request(Request::Hello {
            protocol_version: PROTOCOL_VERSION,
        });

        let deadline = Instant::now() + Duration::from_secs(5);
        let (tick_hz, assigned_entity_id) = 'handshake: loop {
            anyhow::ensure!(Instant::now() < deadline, "handshake timed out");
            for event in network_handle.try_recv_events() {
                match event {
                    Event::Welcome {
                        tick_hz,
                        assigned_entity_id,
                    } => {
                        break 'handshake (tick_hz, assigned_entity_id);
                    }
                    Event::Rejected { reason } => {
                        anyhow::bail!("connection rejected by server: {reason:?}");
                    }
                    Event::Pong { .. } => {}
                }
            }
            std::thread::sleep(Duration::from_millis(5));
        };

        let presentation_buffer: PresentationBuffer =
            Arc::new(ArcSwap::from_pointee(PresentationState::default()));

        let clock_estimate = Arc::new(ArcSwap::from_pointee(ClockEstimate {
            reference_instant: Instant::now(),
            offset_ticks: 0.0,
        }));

        Ok(Self {
            tick_hz,
            clock_estimate,

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
        let mut clock_sync = ClockSync::new(tick_hz, Instant::now(), self.clock_estimate.clone());
        let mut snapshot_ack = SnapshotAck::new();

        let handle = std::thread::Builder::new()
        .name("blackflower-replica::tick".to_owned())
        .spawn(move || {
            let dt = 1.0 / tick_hz as f32;

            let result = scheduler.start(|tick, _dt| {
                let now = Instant::now();

                let mut latest_ack: Option<Tick> = None;
                tick_network_handle.try_recv_snapshots().for_each(|snapshot| {
                    let snap_tick = Tick::from(snapshot.tick);
                    world.merge(&snapshot, snap_tick);
                    let ack = Tick::from(snapshot.ack);
                    latest_ack = Some(latest_ack.map_or(ack, |cur| cur.max(ack)));
                    snapshot_ack.record(snapshot.tick);
                    clock_sync.seed_from_snapshot(snap_tick, now);
                });

                tick_network_handle.try_recv_events().for_each(|event| match event {
                    Event::Pong { client_send_ns, server_tick } => {
                        clock_sync.on_pong(client_send_ns, server_tick, now);
                    }
                    Event::Welcome { .. } => {
                        warn!("spurious Welcome in tick thread — ignored");
                    }
                    Event::Rejected { .. } => {
                        warn!("spurious Rejected in tick thread — ignored");
                    }
                });

                if tick.as_u64() % tick_hz == 0 {
                    tick_network_handle.try_send_request(clock_sync.make_ping(now));
                }

                let input_cmd = tick_input_handle.command(tick);
                if tick.as_u64() % tick_hz == 0 {
                    debug!(tick = %tick, buttons = ?InputButtons::from_bits(input_cmd.buttons).unwrap_or_default(), "input command");
                }

                #[allow(clippy::excessive_nesting)]
                if let Some(local) = prediction.local_player() {
                    if let (Some(ack), Some(authoritative)) =
                        (latest_ack, world.transform_of(local))
                    {
                        prediction.reconcile(authoritative, ack, dt);
                    }

                    let buttons = InputButtons::from_bits(input_cmd.buttons).unwrap_or_default();
                    let look = (input_cmd.yaw, input_cmd.pitch);
                    let seed = world.transform_of(local);
                    if let Some(predicted) = prediction.predict(tick, buttons, look, seed, dt) {
                        world.set_transform(local, predicted);
                    }
                }

                tick_network_handle.try_send_command(Command {
                    snapshot_ack_tick: snapshot_ack.ack_tick,
                    snapshot_ack_bits: snapshot_ack.bits,
                    ..input_cmd
                });

                let local = prediction
                    .local_player()
                    .zip(prediction.local_transform());
                let state = Arc::new(world.state(local));
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

    /// Feed a relative mouse motion (already scaled by sensitivity, radians)
    /// into the local view angles.
    pub fn look(&self, dyaw: f32, dpitch: f32) {
        self.input_handle.look(dyaw, dpitch);
    }

    pub fn state(&self, now: Instant) -> RenderState {
        let state = self.presentation_buffer.load_full();
        self.resolve(&state, now)
    }

    fn resolve(&self, state: &PresentationState, now: Instant) -> RenderState {
        let est = self.clock_estimate.load();
        let elapsed = now.duration_since(est.reference_instant).as_secs_f64();
        let target = elapsed.mul_add(self.tick_hz as f64, est.offset_ticks) - INTERP_DELAY_TICKS;

        let mut camera = None;
        let mut entities = Vec::new();
        for (_, ent) in &state.entities {
            match ent {
                // The local (predicted) player drives the camera; its body is
                // not drawn (first-person).
                EntityState::Predicted(t) => camera = Some(*t),
                EntityState::Interpolated(samples) => {
                    if let Some(t) = samples.interpolate(target) {
                        entities.push(t);
                    }
                }
            }
        }
        RenderState {
            camera,
            entities: entities.into_boxed_slice(),
        }
    }
}
