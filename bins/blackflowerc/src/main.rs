use std::{
    net::SocketAddr,
    sync::{Arc, Mutex},
};

use anyhow::Context;
use arc_swap::ArcSwap;
use blackflower_entity::EntityId;
use blackflower_graphics::renderer::Renderer;
use blackflower_input::{InputHandle, components::InputButtons};
use blackflower_math::components::Transform;
use blackflower_prediction::PredictionState;
use blackflower_protocol::{Event, Request};
use blackflower_tick::TickScheduler;
use blackflower_window::WindowHandler;
use blackflower_world::PresentationWorld;
use clap::Parser;
use tracing::{error, info};
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
}

/// Render-ready, owned snapshot published from the tick thread to the
/// render thread. Order is unspecified; key by `EntityId`.
type FrameBuffer = Arc<ArcSwap<Box<[(EntityId, Transform)]>>>;

fn main() -> anyhow::Result<()> {
    let args = Args::parse();

    tracing_subscriber::registry()
        .with(fmt::layer())
        .with(EnvFilter::from_default_env())
        .init();

    let input_handle = Arc::new(InputHandle::default());

    // Shared, lock-free frame buffer: tick thread publishes, render reads.
    let framebuffer: FrameBuffer = Arc::new(ArcSwap::from_pointee(Box::from([])));

    let input_handle_clone = input_handle.clone();
    let framebuffer_clone = framebuffer.clone();
    std::thread::Builder::new()
        .name("blackflowerc::tick".to_owned())
        .spawn(move || {
            let network_handle = Arc::new(
                blackflower_network::client::connect(args.server_addr)
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
                    #[allow(clippy::excessive_nesting)]
                    match event {
                        Event::Welcome { assigned_entity } => {
                            info!(entity = %assigned_entity, "received welcome");
                        }
                    }
                }

                network_handle
                    .try_recv_snapshots()
                    .for_each(|snapshot| world.apply(&snapshot));

                let command = input_handle_clone.command(tick);
                if tick % args.tick_rate_hz == 0 {
                    info!(tick = %tick, input = ?command, "input command");
                }

                if let Some(local) = prediction.local_player() {
                    let buttons = InputButtons::from_bits(command.buttons).unwrap_or_default();
                    let seed = world.transform_of(local);
                    if let Some(predicted) = prediction.predict(tick, buttons, seed, dt) {
                        world.set_transform(local, predicted);
                    }
                }

                network_handle.try_send_command(command);

                // Publish a render-ready extract.
                framebuffer_clone.store(Arc::new(world.extract()));
            })
        })?;

    let app = Arc::new(Mutex::new(App::new(framebuffer, input_handle)));
    blackflower_window::start(args.width, args.height, app)
}

struct App {
    renderer: Option<Renderer>,
    framebuffer: FrameBuffer,
    input_handle: Arc<InputHandle>,
}

impl App {
    const fn new(framebuffer: FrameBuffer, input_handle: Arc<InputHandle>) -> Self {
        Self {
            renderer: None,
            framebuffer,
            input_handle,
        }
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
        if let Some(renderer) = &mut self.renderer {
            renderer.render(&self.framebuffer.load());
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
