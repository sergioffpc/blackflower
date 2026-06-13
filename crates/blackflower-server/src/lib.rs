use anyhow::Context;
use blackflower_entity::EntityId;
use blackflower_input::components::InputButtons;
use blackflower_math::components::Transform;
use blackflower_network::delay::DelayConfig;
use blackflower_network::server::ServerHandle;
use blackflower_network::{ClientId, server};
use blackflower_physics::components::Velocity;
use blackflower_protocol::{Command, Event, Request, Snapshot};
use blackflower_tick::{Tick, TickScheduler};
use blackflower_world::simulation::SimulationWorld;
use hashbrown::HashMap;
use std::net::SocketAddr;
use std::thread::JoinHandle;
use tracing::{debug, error, info};

#[derive(Clone, Copy, Debug, Default)]
pub struct ServerConfig {
    pub tick_rate_hz: u64,
    pub latency_ms: u64,
    pub jitter_ms: u64,
}

pub struct Server {
    world: SimulationWorld,
    tick_rate_hz: u64,
    client_entities: HashMap<ClientId, EntityId>,
    last_processed: HashMap<ClientId, Tick>,

    network_handle: ServerHandle<Command, Snapshot, Request, Event>,
}

impl Server {
    pub fn listen(addr: SocketAddr, config: ServerConfig) -> anyhow::Result<Self> {
        let world = SimulationWorld::default();

        // M3: maps each connected client to the entity it controls. The map
        // grows on Hello (idempotently) and shrinks on disconnect. Entity ids
        // are monotonic and never reused, so a despawned avatar's id can never
        // be inherited by a later client.
        let client_entities: HashMap<ClientId, EntityId> = HashMap::new();

        // M3: per-client ack — the highest client command tick processed for
        // each client, echoed in that client's snapshots for reconciliation.
        // Replaces the single global u64 of M2.
        let last_processed: HashMap<ClientId, Tick> = HashMap::new();

        let network_handle: ServerHandle<Command, Snapshot, Request, Event> = server::start(
            addr,
            DelayConfig::from_millis(config.latency_ms, config.jitter_ms),
        )
        .context("starting server")?;

        Ok(Self {
            world,
            tick_rate_hz: config.tick_rate_hz,
            client_entities,
            last_processed,

            network_handle,
        })
    }

    pub fn start(mut self) -> anyhow::Result<JoinHandle<()>> {
        std::thread::Builder::new()
            .name("blackflower-server::tick".to_owned())
            .spawn(move || {
                let result = TickScheduler::new(self.tick_rate_hz).start(|tick, elapsed| {
                    // Drain each receiver into an owned buffer first: the
                    // iterators borrow `self.network_handle`, so the borrow
                    // must end before the `&mut self` handlers run.
                    let requests: Vec<_> = self.network_handle.try_recv_requests().collect();
                    for (client_id, request) in requests {
                        self.on_request(client_id, &request);
                    }

                    let commands: Vec<_> = self.network_handle.try_recv_commands().collect();
                    for (client_id, command) in commands {
                        self.on_command(client_id, &command, elapsed.as_secs_f32());
                    }

                    let disconnects: Vec<_> = self.network_handle.try_recv_disconnects().collect();
                    for client_id in disconnects {
                        self.last_processed.remove(&client_id);
                        #[allow(clippy::excessive_nesting)]
                        if let Some(entity) = self.client_entities.remove(&client_id) {
                            self.world.despawn(entity);
                        }
                    }

                    self.on_tick(tick, elapsed.as_secs_f32());

                    for (client_id, _entity) in &self.client_entities {
                        let ack = self.last_processed.get(client_id).copied().unwrap_or(Tick::ZERO);
                        let snapshot = self.world.snapshot(tick, ack);

                        #[allow(clippy::excessive_nesting)]
                        if tick.as_u64() % self.tick_rate_hz == 0 {
                            debug!(client_id = ?client_id, tick = %tick, snapshot = ?snapshot, "world snapshot");
                        }

                        self.network_handle.try_send_snapshot_to(*client_id, snapshot);
                    }
                });
                if let Err(error) = result {
                    error!(%error, "tick thread terminated");
                }
            }).map_err(Into::into)
    }

    fn on_request(&mut self, client_id: ClientId, request: &Request) {
        match request {
            Request::Hello => {
                let assigned_entity = *self
                    .client_entities
                    .entry(client_id)
                    .or_insert_with(|| self.world.spawn((Transform::identity(),)));
                info!(client = %client_id, entity = %assigned_entity, "assigned entity");
                self.network_handle.try_send_event_to(
                    client_id,
                    Event::Welcome {
                        tick_rate_hz: self.tick_rate_hz,
                        assigned_entity: assigned_entity.into(),
                    },
                );
            }
        }
    }

    fn on_command(&mut self, client_id: ClientId, command: &Command, dt: f32) {
        // Record the highest client tick processed for this client.
        self.last_processed
            .entry(client_id)
            .and_modify(|t| *t = (*t).max(command.tick.into()))
            .or_insert(command.tick.into());

        // Apply to the entity this client controls. A command from a
        // client with no avatar (e.g. arrived before Hello, or after
        // disconnect cleanup) is dropped.
        let Some(&entity) = self.client_entities.get(&client_id) else {
            return;
        };
        if let Ok(mut transform) = self.world.transform_mut(entity) {
            blackflower_gameplay::systems::apply_player_movement(
                &mut transform,
                InputButtons::from_bits(command.buttons).unwrap_or_default(),
                dt,
            );
        }
    }

    fn on_tick(&mut self, _tick: Tick, dt: f32) {
        blackflower_physics::systems::integrate_movement(
            self.world.query_mut::<(&mut Transform, &Velocity)>(),
            dt,
        );
    }
}
