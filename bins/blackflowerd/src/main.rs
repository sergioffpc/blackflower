use std::net::SocketAddr;

use anyhow::Context;
use blackflower_input::InputSnapshot;
use blackflower_math::components::Transform;
use blackflower_network::server;
use blackflower_tick::TickScheduler;
use blackflower_world::SimulationWorld;
use clap::Parser;
use tracing::info;
use tracing_subscriber::{EnvFilter, fmt, layer::SubscriberExt, util::SubscriberInitExt};

mod systems;

#[derive(Parser)]
#[command(version, about, long_about = None)]
struct Args {
    #[arg(long, default_value_t = 60)]
    tick_rate_hz: u64,

    /// Address the server binds to.
    #[arg(long, default_value = "0.0.0.0:3512")]
    bind_addr: SocketAddr,
}

fn main() -> anyhow::Result<()> {
    let args = Args::parse();

    tracing_subscriber::registry()
        .with(fmt::layer())
        .with(EnvFilter::from_default_env())
        .init();

    let server_handle = server::start(args.bind_addr).context("starting server")?;

    let mut world = SimulationWorld::default();
    world.spawn((Transform::identity(),));

    TickScheduler::new(args.tick_rate_hz).start(|tick, elapsed| {
        for input_snapshot in server_handle.try_recv_input_snapshots() {
            let i: InputSnapshot = input_snapshot;
        }

        systems::integrate_movement(&mut world, elapsed.as_secs_f32());

        let world_snapshot = world.snapshot(tick);
        if tick % args.tick_rate_hz == 0 {
            info!(tick = %tick, world = ?world_snapshot, "world snapshot");
        }

        server_handle.try_send_world_snapshot(world_snapshot);
    })
}
