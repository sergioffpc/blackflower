use std::net::SocketAddr;

use anyhow::Context;
use blackflower_core::ecs::PresentationWorld;
use blackflower_net::client;
use clap::Parser;
use tracing_subscriber::{EnvFilter, fmt, layer::SubscriberExt, util::SubscriberInitExt};

#[derive(Parser)]
#[command(version, about, long_about = None)]
struct Args {
    /// Server address to bind/connect to.
    #[arg(long, default_value = "127.0.0.1:3512")]
    server_addr: SocketAddr,
}

fn main() -> anyhow::Result<()> {
    let args = Args::parse();

    tracing_subscriber::registry()
        .with(fmt::layer())
        .with(EnvFilter::from_default_env())
        .init();

    let client_handle = client::connect(args.server_addr).context("connecting to server")?;

    let mut world = PresentationWorld::default();
    #[allow(clippy::infinite_loop)]
    loop {
        for snapshot in &client_handle.drain_snapshots() {
            world.apply(snapshot);
        }
    }
}
