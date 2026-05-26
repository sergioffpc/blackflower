use std::time::Instant;

use blackflower_core::{
    ecs::{
        World,
        components::{Transform, Velocity},
    },
    math::Vec3,
    time::{TICK_DT_SECS, TICK_DURATION, TICK_HZ, Tick},
};
use clap::Parser;
use tracing::{info, warn};
use tracing_subscriber::{EnvFilter, fmt, layer::SubscriberExt, util::SubscriberInitExt};

#[derive(Parser)]
#[command(version, about, long_about = None)]
struct Args;

fn main() -> anyhow::Result<()> {
    let _args = Args::parse();

    tracing_subscriber::registry()
        .with(fmt::layer())
        .with(EnvFilter::from_default_env())
        .init();

    info!(
        tick_hz = TICK_HZ,
        tick_dt_secs = TICK_DT_SECS,
        tick_duration_us = u64::try_from(TICK_DURATION.as_micros())?,
        "ticker"
    );

    let mut world = World::new();
    let _entity = world.spawn((Transform::identity(), Velocity(Vec3::new(1.0, 0.0, 0.0))));

    let mut current_tick = Tick::ZERO;
    let mut next_tick_instant = Instant::now() + TICK_DURATION;

    #[allow(clippy::infinite_loop, reason = "server runs until SIGTERM")]
    loop {
        let current_tick_instant = Instant::now();

        blackflower_core::ecs::systems::integrate_movement(&mut world, TICK_DT_SECS);
        for transform in &mut world.query::<&Transform>() {
            info!(
                tick = %current_tick,
                x = transform.translation.x,
                y = transform.translation.y,
                z = transform.translation.z,
                "snapshot"
            );
        }

        let now = Instant::now();
        if now < next_tick_instant {
            std::thread::sleep(next_tick_instant - now);
        } else {
            let overrun = now - current_tick_instant;
            warn!(
                tick = %current_tick,
                overrun_us = u64::try_from(overrun.as_micros())?,
                "ticker overran"
            );
        }

        current_tick = current_tick.next();
        next_tick_instant += TICK_DURATION;
    }
}
