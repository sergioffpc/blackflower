use std::net::SocketAddr;

use anyhow::Context;
use blackflower_core::{
    ecs::{
        SimulationWorld,
        components::{Transform, Velocity},
    },
    math::Vec3,
    time::TickScheduler,
};
use blackflower_net::server;
use clap::Parser;
use tracing_subscriber::{EnvFilter, fmt, layer::SubscriberExt, util::SubscriberInitExt};

#[derive(Parser)]
#[command(version, about, long_about = None)]
struct Args {
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
    world.spawn((Transform::identity(), Velocity(Vec3::new(1.0, 0.0, 0.0))));

    TickScheduler::new(60).start(|tick| {
        blackflower_core::ecs::systems::integrate_movement(&mut world, 0.001);

        let snapshot = world.snapshot(tick);
        server_handle.send_snapshot(snapshot);
    })
}
