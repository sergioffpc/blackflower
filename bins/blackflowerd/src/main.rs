use std::net::SocketAddr;

use anyhow::Context;
use blackflower_entity::EntityId;
use blackflower_input::components::InputButtons;
use blackflower_math::components::Transform;
use blackflower_network::{
    ClientId,
    delay::DelayConfig,
    server::{self, ServerHandle},
};
use blackflower_physics::components::Velocity;
use blackflower_protocol::{Command, Event, Request, Snapshot};
use blackflower_tick::{Tick, TickScheduler};
use blackflower_world::SimulationWorld;
use clap::Parser;
use hashbrown::HashMap;
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

    let server_handle: ServerHandle<Command, Snapshot, Request, Event> = server::start(
        args.bind_addr,
        DelayConfig::from_millis(args.fake_latency_ms, args.fake_jitter_ms),
    )
    .context("starting server")?;

    let mut world = SimulationWorld::default();

    // M3: maps each connected client to the entity it controls. The map
    // grows on Hello (idempotently) and shrinks on disconnect. Entity ids
    // are monotonic and never reused, so a despawned avatar's id can never
    // be inherited by a later client.
    let mut client_entities: HashMap<ClientId, EntityId> = HashMap::new();

    // M3: per-client ack — the highest client command tick processed for
    // each client, echoed in that client's snapshots for reconciliation.
    // Replaces the single global u64 of M2.
    let mut last_processed: HashMap<ClientId, Tick> = HashMap::new();

    TickScheduler::new(args.tick_rate_hz).start(|tick, elapsed| {
        let dt = elapsed.as_secs_f32();

        for (client_id, request) in server_handle.try_recv_requests() {
            match request {
                Request::Hello => {
                    let assigned_entity = *client_entities
                        .entry(client_id)
                        .or_insert_with(|| world.spawn((Transform::identity(),)));
                    server_handle.try_send_event_to(
                        client_id,
                        Event::Welcome {
                            assigned_entity: assigned_entity.into(),
                        },
                    );
                }
            }
        }

        for (client_id, command) in server_handle.try_recv_commands() {
            // Record the highest client tick processed for this client.
            last_processed
                .entry(client_id)
                .and_modify(|t| *t = (*t).max(command.tick.into()))
                .or_insert(command.tick.into());

            // Apply to the entity this client controls. A command from a
            // client with no avatar (e.g. arrived before Hello, or after
            // disconnect cleanup) is dropped.
            let Some(&entity) = client_entities.get(&client_id) else {
                continue;
            };
            if let Ok(mut transform) = world.transform_mut(entity) {
                blackflower_gameplay::systems::apply_player_movement(
                    &mut transform,
                    InputButtons::from_bits(command.buttons).unwrap_or_default(),
                    dt,
                );
            }
        }

        for client_id in server_handle.try_recv_disconnects() {
            last_processed.remove(&client_id);
            if let Some(entity) = client_entities.remove(&client_id) {
                world.despawn(entity);
            }
        }

        blackflower_physics::systems::integrate_movement(
            world.query_mut::<(&mut Transform, &Velocity)>(),
            dt,
        );

        for (client_id, _entity) in &client_entities {
            let ack = last_processed.get(client_id).copied().unwrap_or(Tick::ZERO);
            let snapshot = world.snapshot(tick, ack);
            server_handle.try_send_snapshot_to(*client_id, snapshot);
        }
    })
}
