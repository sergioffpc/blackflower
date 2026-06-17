use std::{net::SocketAddr, path::PathBuf};

use anyhow::Context;
use blackflower_authority::{Authority, AuthorityConfig};
use blackflower_world::arena::Arena;
use clap::Parser;
use serde::Deserialize;
use tracing_subscriber::{EnvFilter, fmt, layer::SubscriberExt, util::SubscriberInitExt};

#[derive(Parser)]
#[command(version, about, long_about = None)]
struct Args {
    /// Path to the server configuration file (TOML).
    #[arg(long, default_value = "assets/blackflowerd.toml")]
    config: PathBuf,

    #[arg(long, default_value_t = 60)]
    tick_hz: u64,

    #[arg(long, default_value_t = 64)]
    max_clients: usize,

    #[arg(long, default_value = "0.0.0.0:3512")]
    bind_addr: SocketAddr,

    #[arg(long, default_value_t = 0)]
    fake_latency_ms: u64,

    #[arg(long, default_value_t = 0)]
    fake_jitter_ms: u64,
}

/// Server configuration loaded from the TOML file given by `--config`.
#[derive(Debug, Deserialize)]
struct ServerConfig {
    /// Path to the arena/map file (RON) to load.
    arena: PathBuf,
    /// Path to the WASM game-plugin component. Omit to run without a plugin.
    #[serde(default)]
    plugin: Option<PathBuf>,
}

impl ServerConfig {
    fn load(path: &std::path::Path) -> anyhow::Result<Self> {
        let src = std::fs::read_to_string(path)
            .with_context(|| format!("reading server config {}", path.display()))?;
        toml::from_str(&src).with_context(|| format!("parsing server config {}", path.display()))
    }
}

fn main() -> anyhow::Result<()> {
    let args = Args::parse();

    tracing_subscriber::registry()
        .with(fmt::layer())
        .with(EnvFilter::from_default_env())
        .init();

    let config = ServerConfig::load(&args.config)?;
    let arena = Arena::load(&config.arena)?;
    let authority_config = AuthorityConfig {
        tick_hz: args.tick_hz,
        max_clients: args.max_clients,
        latency_ms: args.fake_latency_ms,
        jitter_ms: args.fake_jitter_ms,
        arena,
        plugin_path: config.plugin,
    };
    let authority = Authority::listen(args.bind_addr, authority_config)?;
    authority
        .start()?
        .join()
        .map_err(|_| anyhow::anyhow!("server tick thread panicked"))
}
