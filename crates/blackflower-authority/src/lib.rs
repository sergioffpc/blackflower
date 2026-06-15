use anyhow::Context;
use blackflower_entity::EntityId;
use blackflower_input::components::InputButtons;
use blackflower_math::components::Transform;
use blackflower_network::server::ServerHandle;
use blackflower_network::server::{self, TransportConfig};
use blackflower_network::{connection::ConnectionId, delay::DelayConfig};
use blackflower_physics::components::Velocity;
use blackflower_protocol::{Command, Event, Request, Snapshot};
use blackflower_tick::{Tick, TickScheduler};
use blackflower_world::simulation::SimulationWorld;
use hashbrown::HashMap;
use std::net::SocketAddr;
use std::sync::Arc;
use std::thread::JoinHandle;
use tracing::{debug, error, info};

#[derive(Clone, Copy, Debug, Default)]
pub struct AuthorityConfig {
    pub tick_hz: u64,
    pub latency_ms: u64,
    pub jitter_ms: u64,
}

pub struct Authority {
    world: SimulationWorld,
    tick_hz: u64,
    conn_entities: HashMap<ConnectionId, EntityId>,
    last_processed: HashMap<ConnectionId, Tick>,

    network_handle: ServerHandle<Command, Snapshot, Request, Event>,
}

impl Authority {
    pub fn listen(addr: SocketAddr, config: AuthorityConfig) -> anyhow::Result<Self> {
        let world = SimulationWorld::default();

        let conn_entities: HashMap<ConnectionId, EntityId> = HashMap::new();
        let last_processed: HashMap<ConnectionId, Tick> = HashMap::new();

        let network_handle: ServerHandle<Command, Snapshot, Request, Event> = server::start(
            addr,
            TransportConfig::default(),
            DelayConfig::from_millis(config.latency_ms, config.jitter_ms),
        )
        .context("starting server")?;

        Ok(Self {
            world,
            tick_hz: config.tick_hz,
            conn_entities,
            last_processed,

            network_handle,
        })
    }

    pub fn start(mut self) -> anyhow::Result<JoinHandle<()>> {
        std::thread::Builder::new()
            .name("blackflower-authority::tick".to_owned())
            .spawn(move || {
                let result = TickScheduler::new(self.tick_hz).start(|tick, dt| {
                    let requests: Vec<_> = self.network_handle.try_recv_requests().collect();
                    for (conn_id, request) in requests {
                        self.on_request(conn_id, &request);
                    }

                    let commands: Vec<_> = self.network_handle.try_recv_commands().collect();
                    for (conn_id, command) in commands {
                        self.on_command(conn_id, &command, dt.as_secs_f32());
                    }

                    let disconnects: Vec<_> = self.network_handle.try_recv_disconnects().collect();
                    for conn_id in disconnects {
                        self.last_processed.remove(&conn_id);
                        #[allow(clippy::excessive_nesting)]
                        if let Some(entity) = self.conn_entities.remove(&conn_id) {
                            self.world.despawn(entity);
                        }
                    }

                    self.on_tick(tick, dt.as_secs_f32());

                    let world = Arc::new(self.world.snapshot());
                    for (conn_id, _entity) in &self.conn_entities {
                        #[allow(clippy::excessive_nesting)]
                        if tick.as_u64() % self.tick_hz == 0 {
                            debug!(connection_id = ?conn_id, tick = %tick, snapshot = ?world, "world snapshot");
                        }

                        let ack = self.last_processed.get(conn_id).copied().unwrap_or(Tick::ZERO);
                        self.network_handle.try_send_snapshot_to(*conn_id, Snapshot { tick: tick.as_u64(), ack: ack.as_u64(), world: (*world).clone() });
                    }
                });
                if let Err(error) = result {
                    error!(%error, "tick thread terminated");
                }
            }).map_err(Into::into)
    }

    fn on_request(&mut self, conn_id: ConnectionId, request: &Request) {
        match request {
            Request::Hello => {
                let assigned_entity_id = *self
                    .conn_entities
                    .entry(conn_id)
                    .or_insert_with(|| self.world.spawn((Transform::identity(),)));
                info!(client = %conn_id, entity_id = %assigned_entity_id, "assigned entity");
                self.network_handle.try_send_event_to(
                    conn_id,
                    Event::Welcome {
                        tick_hz: self.tick_hz,
                        assigned_entity_id: u64::from(assigned_entity_id),
                    },
                );
            }
        }
    }

    fn on_command(&mut self, conn_id: ConnectionId, command: &Command, dt: f32) {
        let Some(&entity) = self.conn_entities.get(&conn_id) else {
            return;
        };
        self.last_processed
            .entry(conn_id)
            .and_modify(|t| *t = (*t).max(Tick::from(command.tick)))
            .or_insert(Tick::from(command.tick));

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
