use std::net::SocketAddr;

use blackflower_authority::{Authority, AuthorityConfig};
use clap::Parser;
use tracing_subscriber::{EnvFilter, fmt, layer::SubscriberExt, util::SubscriberInitExt};

#[derive(Parser)]
#[command(version, about, long_about = None)]
struct Args {
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

fn main() -> anyhow::Result<()> {
    let args = Args::parse();

    tracing_subscriber::registry()
        .with(fmt::layer())
        .with(EnvFilter::from_default_env())
        .init();

    let authority_config = AuthorityConfig {
        tick_hz: args.tick_hz,
        max_clients: args.max_clients,
        latency_ms: args.fake_latency_ms,
        jitter_ms: args.fake_jitter_ms,
    };
    let authority = Authority::listen(args.bind_addr, authority_config)?;
    authority
        .start()?
        .join()
        .map_err(|_| anyhow::anyhow!("server tick thread panicked"))
}
