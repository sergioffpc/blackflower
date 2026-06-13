use std::net::SocketAddr;

use blackflower_server::{Server, ServerConfig};
use clap::Parser;
use tracing_subscriber::{EnvFilter, fmt, layer::SubscriberExt, util::SubscriberInitExt};

#[derive(Parser)]
#[command(version, about, long_about = None)]
struct Args {
    #[arg(long, default_value_t = 60)]
    tick_rate_hz: u64,

    /// Address the server binds to.
    #[arg(long, default_value = "0.0.0.0:3512")]
    bind_addr: SocketAddr,

    /// Artificial inbound latency (ms) applied to received commands.
    /// Zero disables it. Simulates uplink delay for prediction demos.
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

    let config = ServerConfig {
        tick_rate_hz: args.tick_rate_hz,
        latency_ms: args.fake_latency_ms,
        jitter_ms: args.fake_jitter_ms,
    };
    let server = Server::listen(args.bind_addr, config)?;
    server
        .start()?
        .join()
        .map_err(|_| anyhow::anyhow!("server tick thread panicked"))
}
