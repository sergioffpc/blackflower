use std::net::SocketAddr;

use anyhow::Context;
use blackflower_input::components::InputButtons;
use blackflower_math::components::Transform;
use blackflower_network::server::{self, ServerHandle};
use blackflower_physics::components::Velocity;
use blackflower_protocol::{Command, Event, Request, Snapshot};
use blackflower_tick::{Tick, TickScheduler};
use blackflower_world::SimulationWorld;
use clap::Parser;
use tracing::info;
use tracing_subscriber::{EnvFilter, fmt, layer::SubscriberExt, util::SubscriberInitExt};

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

    let server_handle: ServerHandle<Command, Snapshot, Request, Event> =
        server::start(args.bind_addr).context("starting server")?;

    let mut world = SimulationWorld::default();

    // M2: single client, so a single value suffices. In M3 this becomes
    // a HashMap<ClientId, u64> — each client receives a snapshot carrying
    // its own ack — which also requires per-client snapshots (addressed,
    // not broadcast) and per-client entity tracking. Deferred with the
    // rest of M3's multi-client work.
    let mut last_processed_client_tick = Tick::ZERO;

    TickScheduler::new(args.tick_rate_hz).start(|tick, elapsed| {
        let dt = elapsed.as_secs_f32();

        for (client_id, request) in server_handle.try_recv_requests() {
            match request {
                Request::Hello => {
                    // TODO: Se o cliente reenviar Hello (reconexão, ou bug), o servidor cria uma segunda entidade. Não há idempotência.
                    // Considera rastrear se o client_id já tem entidade atribuída.
                    let assigned_entity = world.spawn((Transform::identity(),));
                    server_handle.try_send_event_to(
                        client_id,
                        Event::Welcome {
                            assigned_entity: assigned_entity.into(),
                        },
                    );
                }
            }
        }

        for (_client_id, command) in server_handle.try_recv_commands() {
            last_processed_client_tick = last_processed_client_tick.max(command.tick.into());

            if let Some(transform) = world.query::<&mut Transform>().iter().next() {
                blackflower_gameplay::systems::apply_player_movement(
                    transform,
                    InputButtons::from_bits(command.buttons).unwrap_or_default(),
                    dt,
                );
            }
        }

        blackflower_physics::systems::integrate_movement(
            world.query_mut::<(&mut Transform, &Velocity)>(),
            dt,
        );

        let snapshot = world.snapshot(tick, last_processed_client_tick);
        if tick % args.tick_rate_hz == 0 {
            info!(tick = %tick, world = ?snapshot, "world snapshot");
        }

        server_handle.try_send_snapshot(snapshot);
    })
}
