use anyhow::Context;
use blackflower_entity::EntityId;
use blackflower_input::components::InputButtons;
use blackflower_math::components::Transform;
use blackflower_network::server::ServerHandle;
use blackflower_network::server::{self, TransportConfig};
use blackflower_network::{connection::ConnectionId, delay::DelayConfig};
use blackflower_physics::components::Velocity;
use blackflower_protocol::{
    Command, EntityDelta, EntitySnapshot, Event, PROTOCOL_VERSION, RejectReason, Request,
    WorldDelta, WorldSnapshot,
};
use blackflower_tick::{Tick, TickScheduler};
use blackflower_world::simulation::SimulationWorld;
use hashbrown::HashMap;
use std::net::SocketAddr;
use std::sync::Arc;
use std::thread::JoinHandle;
use std::time::Duration;
use tracing::{debug, error, info, warn};

const RING_SIZE: usize = 32;
const ZOMBIE_TTL_SECS: u64 = 5;

#[derive(Clone, Copy, Debug, Default)]
pub struct AuthorityConfig {
    pub tick_hz: u64,
    pub max_clients: usize,
    pub latency_ms: u64,
    pub jitter_ms: u64,
}

enum SlotState {
    /// Connection established; waiting for `Request::Hello`.
    Handshake,
    /// Hello received and validated; entity active in the world.
    Playing {
        entity: EntityId,
        last_processed: Tick,
        baseline_tick: u64,
    },
    /// Connection dropped; entity held until `until` for graceful cleanup.
    Zombie { entity: EntityId, until: Tick },
}

/// Fixed-size ring of the last `RING_SIZE` world snapshots, keyed by tick.
struct SnapshotRing {
    entries: [Option<(u64, WorldSnapshot)>; RING_SIZE],
}

impl Default for SnapshotRing {
    fn default() -> Self {
        Self {
            entries: std::array::from_fn(|_| None),
        }
    }
}

impl SnapshotRing {
    fn insert(&mut self, tick: u64, snapshot: WorldSnapshot) {
        self.entries[tick as usize % RING_SIZE] = Some((tick, snapshot));
    }

    fn get(&self, tick: u64) -> Option<&WorldSnapshot> {
        if tick == 0 {
            return None;
        }
        let (stored_tick, snapshot) = self.entries[tick as usize % RING_SIZE].as_ref()?;
        (*stored_tick == tick).then_some(snapshot)
    }
}

pub struct Authority {
    world: SimulationWorld,
    tick_hz: u64,
    max_clients: usize,
    ring: SnapshotRing,
    slots: HashMap<ConnectionId, SlotState>,
    network_handle: ServerHandle<Command, WorldDelta, Request, Event>,
}

impl Authority {
    pub fn listen(addr: SocketAddr, config: AuthorityConfig) -> anyhow::Result<Self> {
        let world = SimulationWorld::default();

        let network_handle: ServerHandle<Command, WorldDelta, Request, Event> = server::start(
            addr,
            TransportConfig::default(),
            DelayConfig::from_millis(config.latency_ms, config.jitter_ms),
        )
        .context("starting server")?;

        Ok(Self {
            world,
            tick_hz: config.tick_hz,
            max_clients: config.max_clients,
            ring: SnapshotRing::default(),
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
        let connects: Vec<_> = self.network_handle.try_recv_connects().collect();
        for conn_id in connects {
            self.slots.entry(conn_id).or_insert(SlotState::Handshake);
            info!(client = %conn_id, "client connected");
        }

        let requests: Vec<_> = self.network_handle.try_recv_requests().collect();
        for (conn_id, request) in requests {
            self.on_request(conn_id, &request, tick);
        }

        // One command per client per tick: keep the highest-tick command.
        let pending = self.drain_commands();
        for (conn_id, command) in &pending {
            self.on_command(*conn_id, command, dt.as_secs_f32());
        }

        let disconnects: Vec<_> = self.network_handle.try_recv_disconnects().collect();
        for conn_id in disconnects {
            self.on_disconnect(conn_id, tick);
        }

        self.expire_zombies(tick);
        self.on_tick(tick, dt.as_secs_f32());
        self.broadcast_snapshots(tick);
    }

    fn on_disconnect(&mut self, conn_id: ConnectionId, tick: Tick) {
        let Some(state) = self.slots.remove(&conn_id) else {
            return;
        };
        match state {
            SlotState::Playing { entity, .. } => {
                let until = Tick::from(tick.as_u64() + self.tick_hz * ZOMBIE_TTL_SECS);
                info!(client = %conn_id, entity_id = %entity, "client disconnected; holding entity");
                self.slots
                    .insert(conn_id, SlotState::Zombie { entity, until });
            }
            SlotState::Handshake => {
                info!(client = %conn_id, "handshake connection dropped");
            }
            SlotState::Zombie { .. } => {}
        }
    }

    fn expire_zombies(&mut self, tick: Tick) {
        let expired: Vec<ConnectionId> = self
            .slots
            .iter()
            .filter_map(|(&id, state)| {
                if let SlotState::Zombie { until, .. } = state {
                    (tick >= *until).then_some(id)
                } else {
                    None
                }
            })
            .collect();

        for conn_id in expired {
            if let Some(SlotState::Zombie { entity, .. }) = self.slots.remove(&conn_id) {
                self.world.despawn(entity);
                info!(client = %conn_id, entity_id = %entity, "zombie expired; entity despawned");
            }
        }
    }

    fn broadcast_snapshots(&mut self, tick: Tick) {
        let current = Arc::new(self.world.snapshot());
        self.ring.insert(tick.as_u64(), (*current).clone());

        let clients: Vec<(ConnectionId, u64, u64)> = self
            .slots
            .iter()
            .filter_map(|(&id, state)| {
                if let SlotState::Playing {
                    last_processed,
                    baseline_tick,
                    ..
                } = state
                {
                    Some((id, *baseline_tick, last_processed.as_u64()))
                } else {
                    None
                }
            })
            .collect();

        for (conn_id, baseline_tick, ack) in clients {
            let baseline = self.ring.get(baseline_tick);
            let snapshot = build_delta(&current, baseline, baseline_tick, tick, ack);
            if tick.as_u64().is_multiple_of(self.tick_hz) {
                debug!(connection_id = ?conn_id, %tick, baseline_tick, "sending snapshot");
            }
            self.network_handle.try_send_snapshot_to(conn_id, snapshot);
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
            Request::Hello { protocol_version } => self.on_hello(conn_id, *protocol_version),
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

    fn on_hello(&mut self, conn_id: ConnectionId, protocol_version: u32) {
        if protocol_version != PROTOCOL_VERSION {
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

        // Idempotent re-Hello: already playing, just resend Welcome.
        if let Some(SlotState::Playing { entity, .. }) = self.slots.get(&conn_id) {
            let entity_id = u64::from(*entity);
            self.network_handle.try_send_event_to(
                conn_id,
                Event::Welcome {
                    tick_hz: self.tick_hz,
                    assigned_entity_id: entity_id,
                },
            );
            return;
        }

        if !matches!(self.slots.get(&conn_id), Some(SlotState::Handshake)) {
            warn!(client = %conn_id, "Hello on unexpected slot state — ignored");
            return;
        }

        let playing = self
            .slots
            .values()
            .filter(|s| matches!(s, SlotState::Playing { .. }))
            .count();
        if playing >= self.max_clients {
            self.network_handle.try_send_event_to(
                conn_id,
                Event::Rejected {
                    reason: RejectReason::ServerFull,
                },
            );
            return;
        }

        let entity = self.world.spawn((Transform::identity(),));
        self.slots.insert(
            conn_id,
            SlotState::Playing {
                entity,
                last_processed: Tick::ZERO,
                baseline_tick: 0,
            },
        );
        info!(client = %conn_id, entity_id = %entity, "assigned entity");
        self.network_handle.try_send_event_to(
            conn_id,
            Event::Welcome {
                tick_hz: self.tick_hz,
                assigned_entity_id: u64::from(entity),
            },
        );
    }

    fn on_command(&mut self, conn_id: ConnectionId, command: &Command, dt: f32) {
        let Some(SlotState::Playing {
            last_processed,
            baseline_tick,
            entity,
        }) = self.slots.get_mut(&conn_id)
        else {
            return;
        };
        *last_processed = (*last_processed).max(Tick::from(command.tick));
        *baseline_tick = (*baseline_tick).max(highest_acked(
            command.snapshot_ack_tick,
            command.snapshot_ack_bits,
        ));
        let entity = *entity;

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

/// Returns the highest snapshot tick confirmed by the sliding-window ack.
/// Bit `i` set means tick `ack_tick - i` was received; searches from bit 0.
fn highest_acked(ack_tick: u64, bits: u32) -> u64 {
    for i in 0_u32..32 {
        if bits & (1_u32 << i) != 0 {
            return ack_tick.saturating_sub(u64::from(i));
        }
    }
    0
}

fn build_delta(
    current: &WorldSnapshot,
    baseline: Option<&WorldSnapshot>,
    baseline_tick: u64,
    server_tick: Tick,
    ack: u64,
) -> WorldDelta {
    let Some(base) = baseline else {
        return WorldDelta {
            tick: server_tick.as_u64(),
            ack,
            baseline: 0,
            removed: Box::default(),
            entities: current.entities.iter().map(entity_full_delta).collect(),
        };
    };

    let base_index: HashMap<u64, &EntitySnapshot> =
        base.entities.iter().map(|e| (e.id, e)).collect();
    let curr_ids: hashbrown::HashSet<u64> = current.entities.iter().map(|e| e.id).collect();

    let removed: Box<[u64]> = base
        .entities
        .iter()
        .map(|e| e.id)
        .filter(|id| !curr_ids.contains(id))
        .collect();

    let entities: Box<[EntityDelta]> = current
        .entities
        .iter()
        .filter_map(|curr| entity_delta(curr, base_index.get(&curr.id).copied()))
        .collect();

    WorldDelta {
        tick: server_tick.as_u64(),
        ack,
        baseline: baseline_tick,
        removed,
        entities,
    }
}

const fn entity_full_delta(e: &EntitySnapshot) -> EntityDelta {
    EntityDelta {
        id: e.id,
        translation: Some(e.translation),
        rotation: Some(e.rotation),
    }
}

fn entity_delta(curr: &EntitySnapshot, base: Option<&EntitySnapshot>) -> Option<EntityDelta> {
    let Some(base) = base else {
        return Some(entity_full_delta(curr));
    };
    let translation =
        field_changed(&curr.translation, &base.translation).then_some(curr.translation);
    let rotation = field_changed(&curr.rotation, &base.rotation).then_some(curr.rotation);
    (translation.is_some() || rotation.is_some()).then_some(EntityDelta {
        id: curr.id,
        translation,
        rotation,
    })
}

/// Bit-exact change detection via `f32::to_bits`.
fn field_changed(a: &[f32], b: &[f32]) -> bool {
    a.iter()
        .zip(b.iter())
        .any(|(x, y)| x.to_bits() != y.to_bits())
}
