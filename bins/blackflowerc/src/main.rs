use std::{
    net::SocketAddr,
    sync::{Arc, Mutex},
    time::Instant,
};

use blackflower_client::{Client, ClientConfig};
use blackflower_graphics::renderer::Renderer;
use blackflower_input::components::InputButtons;
use blackflower_window::WindowHandler;
use clap::Parser;
use tracing::error;
use tracing_subscriber::{EnvFilter, fmt, layer::SubscriberExt, util::SubscriberInitExt};

#[derive(Parser)]
#[command(version, about, long_about = None)]
struct Args {
    #[arg(long, default_value_t = 1280)]
    width: u32,

    #[arg(long, default_value_t = 720)]
    height: u32,

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

fn main() -> anyhow::Result<()> {
    let args = Args::parse();

    tracing_subscriber::registry()
        .with(fmt::layer())
        .with(EnvFilter::from_default_env())
        .init();

    let config = ClientConfig {
        latency_ms: args.fake_latency_ms,
        jitter_ms: args.fake_jitter_ms,
    };
    let mut client = Client::connect(args.server_addr, config)?;
    client.start()?;

    let app = Arc::new(Mutex::new(App::new(client)));
    blackflower_window::start(args.width, args.height, app)
}

struct App {
    client: Client,
    renderer: Option<Renderer>,
}

impl App {
    const fn new(client: Client) -> Self {
        Self {
            client,
            renderer: None,
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
        self.client.clear_buttons();
    }

    fn on_draw(&mut self) {
        let transforms = self.client.state(Instant::now());
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
        self.client.press_button(button);
    }

    fn on_key_up(&mut self, key: &str) {
        let button = match key {
            "W" => InputButtons::FORWARD,
            "S" => InputButtons::BACKWARD,
            "A" => InputButtons::LEFT,
            "D" => InputButtons::RIGHT,
            _ => return,
        };
        self.client.release_button(button);
    }
}
