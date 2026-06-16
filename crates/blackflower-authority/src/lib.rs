use anyhow::Context;
use blackflower_entity::EntityId;
use blackflower_input::components::InputButtons;
use blackflower_math::components::Transform;
use blackflower_network::server::ServerHandle;
use blackflower_network::server::{self, TransportConfig};
use blackflower_network::{connection::ConnectionId, delay::DelayConfig};
use blackflower_physics::components::Velocity;
use blackflower_protocol::{Command, Event, RejectReason, Request, Snapshot, PROTOCOL_VERSION};
use blackflower_tick::{Tick, TickScheduler};
use blackflower_world::simulation::SimulationWorld;
use hashbrown::HashMap;
use std::net::SocketAddr;
use std::sync::Arc;
use std::thread::JoinHandle;
use std::time::Duration;
use tracing::{debug, error, info};

#[derive(Clone, Copy, Debug, Default)]
pub struct AuthorityConfig {
    pub tick_hz: u64,
    pub max_clients: usize,
    pub latency_ms: u64,
    pub jitter_ms: u64,
}

struct Slot {
    entity: EntityId,
    last_processed: Tick,
}

pub struct Authority {
    world: SimulationWorld,
    tick_hz: u64,
    max_clients: usize,
    slots: HashMap<ConnectionId, Slot>,

    network_handle: ServerHandle<Command, Snapshot, Request, Event>,
}

impl Authority {
    pub fn listen(addr: SocketAddr, config: AuthorityConfig) -> anyhow::Result<Self> {
        let world = SimulationWorld::default();

        let network_handle: ServerHandle<Command, Snapshot, Request, Event> = server::start(
            addr,
            TransportConfig::default(),
            DelayConfig::from_millis(config.latency_ms, config.jitter_ms),
        )
        .context("starting server")?;

        Ok(Self {
            world,
            tick_hz: config.tick_hz,
            max_clients: config.max_clients,
            slots: HashMap::new(),

            network_handle,
        })
    }

    pub fn start(mut self) -> anyhow::Result<JoinHandle<()>> {
        std::thread::Builder::new()
            .name("blackflower-authority::tick".to_owned())
            .spawn(move || {
                let result = TickScheduler::new(self.tick_hz).start(|tick, dt| {
                    self.do_tick(tick, dt);
                });
                if let Err(error) = result {
                    error!(%error, "tick thread terminated");
                }
            })
            .map_err(Into::into)
    }

    fn do_tick(&mut self, tick: Tick, dt: Duration) {
        let requests: Vec<_> = self.network_handle.try_recv_requests().collect();
        for (conn_id, request) in requests {
            self.on_request(conn_id, &request, tick);
        }

        // One command per client per tick: keep the highest-tick command
        // from each burst so jitter or command spam cannot advance the
        // simulation more than one step per tick.
        let pending = self.drain_commands();
        for (conn_id, command) in &pending {
            self.on_command(*conn_id, command, dt.as_secs_f32());
        }

        let disconnects: Vec<_> = self.network_handle.try_recv_disconnects().collect();
        for conn_id in disconnects {
            if let Some(slot) = self.slots.remove(&conn_id) {
                self.world.despawn(slot.entity);
            }
        }

        self.on_tick(tick, dt.as_secs_f32());
        self.broadcast_snapshots(tick);
    }

    fn broadcast_snapshots(&self, tick: Tick) {
        let world = Arc::new(self.world.snapshot());
        for (conn_id, slot) in &self.slots {
            if tick.as_u64().is_multiple_of(self.tick_hz) {
                debug!(connection_id = ?conn_id, tick = %tick, snapshot = ?world, "world snapshot");
            }
            self.network_handle.try_send_snapshot_to(
                *conn_id,
                Snapshot {
                    tick: tick.as_u64(),
                    ack: slot.last_processed.as_u64(),
                    world: (*world).clone(),
                },
            );
        }
    }

    fn drain_commands(&self) -> HashMap<ConnectionId, Command> {
        let mut pending: HashMap<ConnectionId, Command> = HashMap::new();
        for (conn_id, command) in self.network_handle.try_recv_commands() {
            let prev = pending.entry(conn_id).or_insert(command);
            if command.tick > prev.tick {
                *prev = command;
            }
        }
        pending
    }

    fn on_request(&mut self, conn_id: ConnectionId, request: &Request, tick: Tick) {
        match request {
            Request::Hello { protocol_version } => {
                if *protocol_version != PROTOCOL_VERSION {
                    self.network_handle.try_send_event_to(
                        conn_id,
                        Event::Rejected {
                            reason: RejectReason::VersionMismatch {
                                server_version: PROTOCOL_VERSION,
                            },
                        },
                    );
                    return;
                }
                if !self.slots.contains_key(&conn_id) && self.slots.len() >= self.max_clients {
                    self.network_handle.try_send_event_to(
                        conn_id,
                        Event::Rejected {
                            reason: RejectReason::ServerFull,
                        },
                    );
                    return;
                }
                let assigned_entity_id = if let Some(slot) = self.slots.get(&conn_id) {
                    slot.entity
                } else {
                    let entity = self.world.spawn((Transform::identity(),));
                    self.slots.insert(
                        conn_id,
                        Slot {
                            entity,
                            last_processed: Tick::ZERO,
                        },
                    );
                    entity
                };
                info!(client = %conn_id, entity_id = %assigned_entity_id, "assigned entity");
                self.network_handle.try_send_event_to(
                    conn_id,
                    Event::Welcome {
                        tick_hz: self.tick_hz,
                        assigned_entity_id: u64::from(assigned_entity_id),
                    },
                );
            }
            Request::Ping { client_send_ns } => {
                self.network_handle.try_send_event_to(
                    conn_id,
                    Event::Pong {
                        client_send_ns: *client_send_ns,
                        server_tick: tick.as_u64(),
                    },
                );
            }
        }
    }

    fn on_command(&mut self, conn_id: ConnectionId, command: &Command, dt: f32) {
        let Some(slot) = self.slots.get_mut(&conn_id) else {
            return;
        };
        slot.last_processed = slot.last_processed.max(Tick::from(command.tick));
        let entity = slot.entity;

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
