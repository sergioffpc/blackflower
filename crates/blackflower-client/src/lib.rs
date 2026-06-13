use anyhow::Context;
use arc_swap::ArcSwap;
use blackflower_entity::EntityId;
use blackflower_input::{InputHandle, components::InputButtons};
use blackflower_math::components::Transform;
use blackflower_network::{
    client::{self, ClientHandle},
    delay::DelayConfig,
};
use blackflower_prediction::PredictionState;
use blackflower_protocol::{Command, Event, Request, Snapshot};
use blackflower_tick::{Tick, TickScheduler};
use blackflower_world::presentation::{PresentationWorld, RenderEntity, RenderState, interpolate};
use std::thread::JoinHandle;
use std::{
    net::SocketAddr,
    sync::Arc,
    time::{Duration, Instant},
};
use tracing::{debug, error};

/// Render-ready, owned payload published from the tick thread to the
/// render thread. Carries per-entity history so the render thread can
/// interpolate remotes against its own clock.
type FrameBuffer = Arc<ArcSwap<RenderState>>;

/// Interpolation delay, in server ticks. 3 ticks ≈ 50 ms at 60 Hz: enough
/// to always bracket the render target with two received snapshots under
/// normal jitter, without adding excessive visual latency to remotes.
const INTERP_DELAY_TICKS: f64 = 2.0;

#[derive(Clone, Copy, Debug, Default)]
pub struct ClientConfig {
    pub latency_ms: u64,
    pub jitter_ms: u64,
}

pub struct Client {
    tick_rate_hz: u64,
    /// Server-time clock anchor: the newest server tick the render thread
    /// has seen, and the local instant it first saw it. Used to project a
    /// fractional "server tick now" each frame.
    clock_anchor_tick: Tick,
    clock_anchor_instant: Instant,

    /// Local player entity assigned by the server in `Welcome`. Used by
    /// `start` to seed the tick thread's `PredictionState`.
    assigned_entity: EntityId,
    framebuffer: FrameBuffer,

    input_handle: Arc<InputHandle>,
    network_handle: Arc<ClientHandle<Command, Snapshot, Request, Event>>,
}

impl Client {
    pub fn connect(addr: SocketAddr, config: ClientConfig) -> anyhow::Result<Self> {
        let input_handle = Arc::new(InputHandle::default());

        let delay = DelayConfig::from_millis(config.latency_ms, config.jitter_ms);
        let network_handle =
            Arc::new(client::connect(addr, delay).context("connecting to server")?);

        network_handle.try_send_request(Request::Hello);
        let Event::Welcome {
            tick_rate_hz,
            assigned_entity,
        } = loop {
            if let Some(welcome @ Event::Welcome { .. }) = network_handle.try_recv_events().next() {
                break welcome;
            }

            std::thread::sleep(Duration::from_millis(5));
        };

        // Shared, lock-free frame buffer: tick thread publishes, render reads.
        let framebuffer: FrameBuffer = Arc::new(ArcSwap::from_pointee(RenderState::default()));

        Ok(Self {
            tick_rate_hz,
            clock_anchor_tick: Tick::ZERO,
            clock_anchor_instant: Instant::now(),

            assigned_entity: EntityId::from(assigned_entity),
            framebuffer,

            input_handle,
            network_handle,
        })
    }

    pub fn start(&mut self) -> anyhow::Result<JoinHandle<()>> {
        let tick_framebuffer = self.framebuffer.clone();
        let tick_network_handle = self.network_handle.clone();
        let tick_input_handle = self.input_handle.clone();
        let tick_rate_hz = self.tick_rate_hz;

        // The presentation world and prediction state live solely on the
        // tick thread — built here and moved into the closure so it owns
        // them outright rather than borrowing through `self` (which the
        // 'static thread bound forbids). The render thread never touches them.
        let mut world = PresentationWorld::default();
        let mut prediction = PredictionState::new(self.assigned_entity);

        std::thread::Builder::new()
        .name("blackflower-client::tick".to_owned())
        .spawn(move || {
            // The simulation step prediction must match the server's, so
            // that identical inputs produce identical transforms.
            let dt = 1.0 / tick_rate_hz as f32;

            let result = TickScheduler::new(tick_rate_hz).start(|tick, _elapsed| {
                let mut latest_ack: Option<Tick> = None;
                tick_network_handle.try_recv_snapshots().for_each(|snapshot| {
                    world.apply(&snapshot);
                    let ack = Tick::from(snapshot.ack);
                    latest_ack = Some(latest_ack.map_or(ack, |cur| cur.max(ack)));
                });

                let command = tick_input_handle.command(tick);
                if tick.as_u64() % tick_rate_hz == 0 {
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

                // Publish: the local player carries its predicted transform;
                // remotes carry sample history for the render thread to
                // interpolate.
                let local = prediction
                    .local_player()
                    .zip(prediction.local_transform());
                let state = Arc::new(world.extract(local));
                if tick.as_u64() % tick_rate_hz == 0 {
                    debug!(tick = %tick, state = ?state, "publish render state");
                }
                tick_framebuffer.store(state);
            });
            if let Err(error) = result {
                error!(%error, "tick thread terminated");
            }
        }).map_err(Into::into)
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

    pub fn state(&mut self, now: Instant) -> Box<[Transform]> {
        let state = self.framebuffer.load_full();
        self.resolve(&state, now).into_boxed_slice()
    }

    /// Project the fractional server tick the render should display now:
    /// the anchored newest tick plus locally-elapsed time, expressed in
    /// ticks. Re-anchors whenever a newer server tick arrives.
    fn server_time_now(&mut self, state: &RenderState, now: Instant) -> f64 {
        if state.latest_server_tick > self.clock_anchor_tick {
            self.clock_anchor_tick = state.latest_server_tick;
            self.clock_anchor_instant = now;
        }
        let elapsed = now.duration_since(self.clock_anchor_instant).as_secs_f64();
        elapsed.mul_add(self.clock_anchor_tick.as_f64(), self.tick_rate_hz as f64)
    }

    /// Resolve every entity to a final transform: predicted locals as-is,
    /// remotes interpolated at `server_time_now - INTERP_DELAY_TICKS`.
    fn resolve(&mut self, state: &RenderState, now: Instant) -> Vec<Transform> {
        let target = self.server_time_now(state, now) - INTERP_DELAY_TICKS;
        state
            .entities
            .iter()
            .filter_map(|(_, render)| match render {
                RenderEntity::Predicted(t) => Some(*t),
                RenderEntity::Interpolated(samples) => interpolate(samples, target),
            })
            .collect()
    }
}
