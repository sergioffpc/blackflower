use std::{
    net::SocketAddr,
    path::{Path, PathBuf},
    sync::{Arc, Mutex},
    time::Instant,
};

use anyhow::Context;
use blackflower_graphics::renderer::Renderer;
use blackflower_input::components::InputButtons;
use blackflower_replica::replica::{Replica, ReplicaConfig};
use blackflower_window::WindowHandler;
use clap::Parser;
use hashbrown::HashMap;
use serde::Deserialize;
use tracing::error;
use tracing_subscriber::{EnvFilter, fmt, layer::SubscriberExt, util::SubscriberInitExt};

#[derive(Parser)]
#[command(version, about, long_about = None)]
struct Args {
    /// Path to the client configuration file (TOML).
    #[arg(long, default_value = "assets/client.toml")]
    config: PathBuf,

    #[arg(long, default_value_t = 1280)]
    width: u32,

    #[arg(long, default_value_t = 720)]
    height: u32,

    #[arg(long, default_value = "127.0.0.1:3512")]
    server_addr: SocketAddr,

    #[arg(long, default_value_t = 0)]
    fake_latency_ms: u64,

    #[arg(long, default_value_t = 0)]
    fake_jitter_ms: u64,
}

/// Client configuration loaded from the TOML file given by `--config`.
#[derive(Debug, Deserialize)]
struct ClientConfig {
    /// Physical key name (as emitted by the window layer) → action name.
    bindings: HashMap<String, String>,
}

impl ClientConfig {
    fn load(path: &Path) -> anyhow::Result<Self> {
        let src = std::fs::read_to_string(path)
            .with_context(|| format!("reading client config {}", path.display()))?;
        toml::from_str(&src).with_context(|| format!("parsing client config {}", path.display()))
    }

    /// Resolve the textual bindings into a key → [`InputButtons`] lookup,
    /// failing on any unknown action name.
    fn resolve(&self) -> anyhow::Result<HashMap<String, InputButtons>> {
        self.bindings
            .iter()
            .map(|(key, action)| {
                InputButtons::from_action(action)
                    .map(|button| (key.clone(), button))
                    .with_context(|| format!("unknown input action {action:?} bound to {key:?}"))
            })
            .collect()
    }
}

fn main() -> anyhow::Result<()> {
    let args = Args::parse();

    tracing_subscriber::registry()
        .with(fmt::layer())
        .with(EnvFilter::from_default_env())
        .init();

    let config = ClientConfig::load(&args.config)?;
    let bindings = config.resolve()?;

    let replica_config = ReplicaConfig {
        latency_ms: args.fake_latency_ms,
        jitter_ms: args.fake_jitter_ms,
    };
    let mut replica = Replica::connect(args.server_addr, replica_config)?;
    replica.start()?;

    let app = Arc::new(Mutex::new(App::new(replica, bindings)));
    blackflower_window::start(args.width, args.height, app)
}

struct App {
    replica: Replica,
    bindings: HashMap<String, InputButtons>,
    renderer: Option<Renderer>,
}

impl App {
    const fn new(replica: Replica, bindings: HashMap<String, InputButtons>) -> Self {
        Self {
            replica,
            bindings,
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
        self.replica.clear_buttons();
    }

    fn on_draw(&mut self) {
        let transforms = self.replica.state(Instant::now());
        if let Some(renderer) = &mut self.renderer {
            renderer.render(&transforms);
        }
    }

    fn on_key_down(&mut self, key: &str) {
        if let Some(&button) = self.bindings.get(key) {
            self.replica.press_button(button);
        }
    }

    fn on_key_up(&mut self, key: &str) {
        if let Some(&button) = self.bindings.get(key) {
            self.replica.release_button(button);
        }
    }
}
