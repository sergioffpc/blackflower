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

#[derive(Parser)]
#[command(version, about, long_about = None)]
pub struct Args {
    /// Path to the client configuration file (TOML).
    #[arg(long, default_value = "assets/blackflowerc.toml")]
    config_path: PathBuf,

    #[arg(long, default_value = "127.0.0.1:3512")]
    server_addr: SocketAddr,

    #[arg(long, default_value_t = 0)]
    fake_latency_ms: u64,

    #[arg(long, default_value_t = 0)]
    fake_jitter_ms: u64,
}

pub fn run_app(args: Args) -> anyhow::Result<()> {
    let replica_config = ReplicaConfig {
        latency_ms: args.fake_latency_ms,
        jitter_ms: args.fake_jitter_ms,
    };
    let mut replica = Replica::connect(args.server_addr, replica_config)?;
    replica.start()?;

    let config = Config::load(args.config_path)?;
    let app = Arc::new(Mutex::new(App::new(&config, replica)?));
    blackflower_window::start(config.window.width, config.window.height, app)
}

const DEFAULT_WIDTH: u32 = 1280;
const DEFAULT_HEIGHT: u32 = 720;

#[derive(Debug, Deserialize)]
struct Config {
    /// Window dimensions; omitted fields fall back to defaults.
    #[serde(default)]
    window: WindowConfig,
    /// Physical key name (as emitted by the window layer) → action name.
    bindings: HashMap<String, String>,
}

#[derive(Debug, Deserialize)]
#[serde(default)]
struct WindowConfig {
    width: u32,
    height: u32,
}

impl Default for WindowConfig {
    fn default() -> Self {
        Self {
            width: DEFAULT_WIDTH,
            height: DEFAULT_HEIGHT,
        }
    }
}

impl Config {
    fn load<P>(path: P) -> anyhow::Result<Self>
    where
        P: AsRef<Path>,
    {
        let src = std::fs::read_to_string(path.as_ref())
            .with_context(|| format!("reading client config {}", path.as_ref().display()))?;
        toml::from_str(&src)
            .with_context(|| format!("parsing client config {}", path.as_ref().display()))
    }

    fn bindings(&self) -> anyhow::Result<HashMap<String, InputButtons>> {
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

struct App {
    replica: Replica,
    bindings: HashMap<String, InputButtons>,
    renderer: Option<Renderer>,
}

impl App {
    fn new(config: &Config, replica: Replica) -> anyhow::Result<Self> {
        let bindings = config.bindings()?;

        Ok(Self {
            replica,
            bindings,
            renderer: None,
        })
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
