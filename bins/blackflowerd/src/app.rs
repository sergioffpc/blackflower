use std::{net::SocketAddr, path::PathBuf};

use blackflower_authority::authority::{Authority, AuthorityConfig};
use blackflower_world::arena::Arena;
use clap::Parser;

#[derive(Parser)]
#[command(version, about, long_about = None)]
pub struct Args {
    #[arg(long)]
    arena_path: PathBuf,

    #[arg(long)]
    plugin_path: Option<PathBuf>,

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

pub fn run_app(args: Args) -> anyhow::Result<()> {
    let arena = Arena::load(args.arena_path)?;

    let authority_config = AuthorityConfig {
        tick_hz: args.tick_hz,

        max_clients: args.max_clients,

        latency_ms: args.fake_latency_ms,
        jitter_ms: args.fake_jitter_ms,
    };
    let authority = Authority::listen(args.bind_addr, authority_config)?;
    authority
        .start(&arena, args.plugin_path)?
        .join()
        .map_err(|_| anyhow::anyhow!("server tick thread panicked"))
}
