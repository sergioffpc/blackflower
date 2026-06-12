use std::{
    net::SocketAddr,
    sync::{Arc, Mutex},
    time::Instant,
};

use anyhow::Context;
use arc_swap::ArcSwap;
use blackflower_graphics::renderer::Renderer;
use blackflower_input::{InputHandle, components::InputButtons};
use blackflower_math::components::Transform;
use blackflower_network::delay::DelayConfig;
use blackflower_prediction::PredictionState;
use blackflower_protocol::{Event, Request};
use blackflower_tick::{Tick, TickScheduler};
use blackflower_window::WindowHandler;
use blackflower_world::{PresentationWorld, RenderEntity, RenderState, interpolate};
use clap::Parser;
use tracing::{debug, error, info};
use tracing_subscriber::{EnvFilter, fmt, layer::SubscriberExt, util::SubscriberInitExt};

#[derive(Parser)]
#[command(version, about, long_about = None)]
struct Args {
    #[arg(long, default_value_t = 1280)]
    width: u32,

    #[arg(long, default_value_t = 720)]
    height: u32,

    #[arg(long, default_value_t = 60)]
    tick_rate_hz: u64,

    #[arg(long, default_value = "127.0.0.1:3512")]
    server_addr: SocketAddr,

    /// Artificial inbound latency (ms) applied to received snapshots.
    /// Zero disables it. Simulates downlink delay for prediction demos.
    #[arg(long, default_value_t = 0)]
    fake_latency_ms: u64,

    /// Jitter (ms) added to `--fake-latency-ms`, uniform in ±jitter.
    /// May reorder packets. Ignored when latency is zero.
    #[arg(long, default_value_t = 0)]
    fake_jitter_ms: u64,
}

/// Render-ready, owned payload published from the tick thread to the
/// render thread. Carries per-entity history so the render thread can
/// interpolate remotes against its own clock.
type FrameBuffer = Arc<ArcSwap<RenderState>>;

fn main() -> anyhow::Result<()> {
    let args = Args::parse();

    tracing_subscriber::registry()
        .with(fmt::layer())
        .with(EnvFilter::from_default_env())
        .init();

    let input_handle = Arc::new(InputHandle::default());

    // Shared, lock-free frame buffer: tick thread publishes, render reads.
    let framebuffer: FrameBuffer = Arc::new(ArcSwap::from_pointee(RenderState::default()));

    let input_handle_clone = input_handle.clone();
    let framebuffer_clone = framebuffer.clone();
    std::thread::Builder::new()
        .name("blackflowerc::tick".to_owned())
        .spawn(move || {
            let network_handle = Arc::new(
                blackflower_network::client::connect(
                    args.server_addr,
                    DelayConfig::from_millis(args.fake_latency_ms, args.fake_jitter_ms),
                )
                .context("connecting to server")?,
            );

            // Initiate the application-level handshake.
            network_handle.try_send_request(Request::Hello);

            // The presentation world lives here, owned solely by the tick
            // thread. The render thread never touches it.
            let mut world = PresentationWorld::default();

            // Prediction is an optional pipeline step: it overwrites the
            // local player's authoritative pose with a locally-predicted
            // one before extraction. Remove this and the world shows only
            // authoritative state.
            let mut prediction = PredictionState::new();

            // The simulation step prediction must match the server's, so
            // that identical inputs produce identical transforms.
            let dt = 1.0 / args.tick_rate_hz as f32;

            TickScheduler::new(args.tick_rate_hz).start(|tick, _elapsed| {
                for event in network_handle.try_recv_events() {
                    match event {
                        Event::Welcome { assigned_entity } => {
                            info!(entity = %assigned_entity, "assigned entity");
                            prediction.assign(assigned_entity.into());
                        }
                    }
                }

                let mut latest_ack: Option<Tick> = None;
                network_handle.try_recv_snapshots().for_each(|snapshot| {
                    world.apply(&snapshot);
                    let ack = Tick::from(snapshot.ack);
                    latest_ack = Some(latest_ack.map_or(ack, |cur| cur.max(ack)));
                });

                let command = input_handle_clone.command(tick);
                if tick.as_u64() % args.tick_rate_hz == 0 {
                    debug!(tick = %tick, buttons = ?InputButtons::from_bits(command.buttons).unwrap_or_default(), "input command");
                }

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

                network_handle.try_send_command(command);

                // Publish: the local player carries its predicted transform;
                // remotes carry sample history for the render thread to
                // interpolate.
                let local = prediction
                    .local_player()
                    .zip(prediction.local_transform());
                let state = Arc::new(world.extract(local));
                if tick.as_u64() % args.tick_rate_hz == 0 {
                    debug!(tick = %tick, state = ?state, "publish render state");
                }
                framebuffer_clone.store(state);
            })
        })?;

    let app = Arc::new(Mutex::new(App::new(
        framebuffer,
        input_handle,
        args.tick_rate_hz,
    )));
    blackflower_window::start(args.width, args.height, app)
}

/// Interpolation delay, in server ticks. 3 ticks ≈ 50 ms at 60 Hz: enough
/// to always bracket the render target with two received snapshots under
/// normal jitter, without adding excessive visual latency to remotes.
const INTERP_DELAY_TICKS: f64 = 2.0;

struct App {
    renderer: Option<Renderer>,
    framebuffer: FrameBuffer,
    input_handle: Arc<InputHandle>,
    tick_hz: f64,
    /// Server-time clock anchor: the newest server tick the render thread
    /// has seen, and the local instant it first saw it. Used to project a
    /// fractional "server tick now" each frame.
    clock_anchor_tick: Tick,
    clock_anchor_instant: Instant,
}

impl App {
    fn new(framebuffer: FrameBuffer, input_handle: Arc<InputHandle>, tick_rate_hz: u64) -> Self {
        Self {
            renderer: None,
            framebuffer,
            input_handle,
            tick_hz: tick_rate_hz as f64,
            clock_anchor_tick: Tick::ZERO,
            clock_anchor_instant: Instant::now(),
        }
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
        elapsed.mul_add(self.clock_anchor_tick.as_f64(), self.tick_hz)
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

impl WindowHandler for App {
    fn on_create(
        &mut self,
        target: Arc<dyn blackflower_window::SurfaceHandle>,
        width: u32,
        height: u32,
    ) {
        let renderer = match Renderer::new_blocking(target, width, height) {
            Ok(r) => r,
            Err(e) => {
                error!(error = %e, "failed to create renderer");
                return;
            }
        };

        self.renderer = Some(renderer);
    }

    fn on_destroy(&mut self) {}

    fn on_resize(&mut self, width: u32, height: u32) {
        if let Some(renderer) = &mut self.renderer {
            renderer.resize(width, height);
        }
    }

    fn on_gained_focus(&mut self) {}

    fn on_lost_focus(&mut self) {
        self.input_handle.clear();
    }

    fn on_draw(&mut self) {
        let state = self.framebuffer.load_full();
        let transforms = self.resolve(&state, Instant::now());
        if let Some(renderer) = &mut self.renderer {
            renderer.render(&transforms);
        }
    }

    fn on_key_down(&mut self, key: &str) {
        let button = match key {
            "W" => InputButtons::FORWARD,
            "S" => InputButtons::BACKWARD,
            "A" => InputButtons::LEFT,
            "D" => InputButtons::RIGHT,
            _ => return,
        };
        self.input_handle.press(button);
    }

    fn on_key_up(&mut self, key: &str) {
        let button = match key {
            "W" => InputButtons::FORWARD,
            "S" => InputButtons::BACKWARD,
            "A" => InputButtons::LEFT,
            "D" => InputButtons::RIGHT,
            _ => return,
        };
        self.input_handle.release(button);
    }
}
