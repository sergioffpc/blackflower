use std::{
    net::SocketAddr,
    sync::{Arc, Mutex},
};

use anyhow::Context;
use blackflower_graphics::renderer::Renderer;
use blackflower_input::{InputHandle, components::InputButtons};
use blackflower_network::client::ClientHandle;
use blackflower_protocol::{Command, Event, Request, Snapshot};
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

fn main() -> anyhow::Result<()> {
    let args = Args::parse();

    tracing_subscriber::registry()
        .with(fmt::layer())
        .with(EnvFilter::from_default_env())
        .init();

    let input_handle = Arc::new(InputHandle::default());
    let network_handle = Arc::new(
        blackflower_network::client::connect(args.server_addr).context("connecting to server")?,
    );

    // Initiate the application-level handshake.
    network_handle.try_send_request(Request::Hello);

    let network_handle_clone = network_handle.clone();
    let input_handle_clone = input_handle.clone();
    std::thread::Builder::new()
        .name("blackflowerc::input".to_owned())
        .spawn(move || {
            TickScheduler::new(args.tick_rate_hz).start(|tick, _elapsed| {
                for event in network_handle_clone.try_recv_events() {
                    #[allow(clippy::excessive_nesting)]
                    match event {
                        Event::Welcome { assigned_entity } => {
                            info!(entity = %assigned_entity, "received welcome");
                        }
                    }
                }

                let command = input_handle_clone.command(tick);
                if tick % args.tick_rate_hz == 0 {
                    info!(tick = %tick, input = ?command, "input command");
                }

                network_handle_clone.try_send_command(command);
            })
        })?;

    let app = Arc::new(Mutex::new(App::new(network_handle, input_handle)));
    blackflower_window::start(args.width, args.height, app)
}

struct App {
    renderer: Option<Renderer>,
    network_handle: Arc<ClientHandle<Command, Snapshot, Request, Event>>,
    input_handle: Arc<InputHandle>,
    world: PresentationWorld,
}

impl App {
    fn new(
        network_handle: Arc<ClientHandle<Command, Snapshot, Request, Event>>,
        input_handle: Arc<InputHandle>,
    ) -> Self {
        Self {
            renderer: None,
            network_handle,
            input_handle,
            world: PresentationWorld::default(),
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
        let Some(renderer) = &mut self.renderer else {
            return;
        };

        self.network_handle
            .try_recv_snapshots()
            .for_each(|snapshot| self.world.apply(&snapshot));
        renderer.render(&self.world);
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
