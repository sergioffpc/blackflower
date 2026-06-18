use anyhow::Context;
use blackflower_gameplay::PLAYER_HALF_EXTENTS;
use blackflower_gameplay::plugin::Plugin;
use blackflower_input::components::InputButtons;
use blackflower_math::components::Transform;
use blackflower_math::{Quat, Vec3};
use blackflower_network::server::ServerHandle;
use blackflower_network::server::{self, TransportConfig};
use blackflower_network::{connection::ConnectionId, delay::DelayConfig};
use blackflower_physics::collision::CollisionWorld;
use blackflower_physics::components::Velocity;
use blackflower_protocol::{
    Command, EntityDelta, EntitySnapshot, Event, PROTOCOL_VERSION, Prop, RejectReason, Request,
    WorldDelta, WorldSnapshot,
};
use blackflower_time::{Tick, TickScheduler};
use blackflower_world::EntityId;
use blackflower_world::arena::{Arena, SpawnPoint};
use blackflower_world::simulation::{EntityProps, SimulationWorld};
use hashbrown::HashMap;
use notify::{
    Event as NotifyEvent, EventKind, RecommendedWatcher, RecursiveMode, Watcher,
    recommended_watcher,
};
use std::net::SocketAddr;
use std::path::{Path, PathBuf};
use std::sync::Arc;
use std::sync::atomic::{AtomicBool, Ordering};
use std::thread::JoinHandle;
use std::time::Duration;
use tracing::{debug, error, info, warn};

const RING_SIZE: usize = 32;
const ZOMBIE_TTL_SECS: u64 = 5;

#[derive(Clone, Copy, Debug)]
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
    tick_hz: u64,
    max_clients: usize,
    ring: SnapshotRing,
    slots: HashMap<ConnectionId, SlotState>,

    network_handle: ServerHandle<Command, WorldDelta, Request, Event>,

    simulation: SimulationWorld,
    collision: CollisionWorld,
    spawn_points: Vec<SpawnPoint>,
    next_spawn: usize,
    plugin: Option<Plugin>,
    /// Source path of the plugin, for hot-reload.
    plugin_path: Option<PathBuf>,
    /// Set by the file watcher when the plugin `.wasm` changes; the tick thread
    /// reloads on the next tick.
    plugin_dirty: Arc<AtomicBool>,
    /// Held only to keep the file watch alive (RAII); dropping it stops the
    /// watch. Never read directly — the watcher pushes to `plugin_dirty`.
    #[allow(dead_code)]
    plugin_watcher: Option<RecommendedWatcher>,
}

impl Authority {
    pub fn listen(addr: SocketAddr, config: AuthorityConfig) -> anyhow::Result<Self> {
        let network_handle: ServerHandle<Command, WorldDelta, Request, Event> = server::start(
            addr,
            TransportConfig::default(),
            DelayConfig::from_millis(config.latency_ms, config.jitter_ms),
        )
        .context("starting server")?;

        Ok(Self {
            tick_hz: config.tick_hz,
            max_clients: config.max_clients,
            ring: SnapshotRing::default(),
            slots: HashMap::new(),
            network_handle,
            simulation: SimulationWorld::default(),
            collision: CollisionWorld::from_solids(std::iter::empty()),
            spawn_points: Vec::new(),
            next_spawn: 0,
            plugin: None,
            plugin_path: None,
            plugin_dirty: Arc::new(AtomicBool::new(false)),
            plugin_watcher: None,
        })
    }

    pub fn start(
        mut self,
        arena: &Arena,
        plugin_path: Option<PathBuf>,
    ) -> anyhow::Result<JoinHandle<()>> {
        self.collision = CollisionWorld::from_solids(
            arena
                .solids()
                .into_iter()
                .map(|a| (Vec3::from_array(a.min), Vec3::from_array(a.max))),
        );
        self.spawn_points = arena.spawn_points();

        if let Some(path) = plugin_path {
            self.plugin = Some(Plugin::load(&path).context("loading plugin")?);
            self.plugin_watcher = spawn_plugin_watcher(&path, self.plugin_dirty.clone());
            self.plugin_path = Some(path);
        }

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
        self.reload_plugin_if_changed();

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
                self.simulation.despawn(entity);
                info!(client = %conn_id, entity_id = %entity, "zombie expired; entity despawned");
            }
        }
    }

    fn broadcast_snapshots(&mut self, tick: Tick) {
        let world_snapshot = Arc::new(self.simulation.snapshot());
        self.ring.insert(tick.as_u64(), (*world_snapshot).clone());

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
            let world_delta = build_delta(&world_snapshot, baseline, baseline_tick, tick, ack);
            if tick.as_u64().is_multiple_of(self.tick_hz) {
                debug!(connection_id = ?conn_id, %tick, baseline_tick, "sending snapshot");
            }
            self.network_handle
                .try_send_snapshot_to(conn_id, world_delta);
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

        let transform = self.next_spawn_transform();
        let initial_props = self
            .plugin
            .as_mut()
            .and_then(|p| p.on_spawn().ok())
            .unwrap_or_default();
        let entity = self
            .simulation
            .spawn((transform, EntityProps(initial_props)));
        self.slots.insert(
            conn_id,
            SlotState::Playing {
                entity,
                last_processed: Tick::ZERO,
                baseline_tick: 0,
            },
        );
        info!(client = %conn_id, entity_id = %entity, spawn = ?transform.translation, "assigned entity");
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
        let buttons = InputButtons::from_bits(command.buttons).unwrap_or_default();

        if let Ok(mut transform) = self.simulation.transform_mut(entity) {
            // Look before move so movement uses the new facing (yaw-relative).
            blackflower_gameplay::systems::apply_player_look(
                &mut transform,
                command.yaw,
                command.pitch,
            );
            let old_pos: [f32; 3] = transform.translation.into();
            blackflower_gameplay::systems::apply_player_movement(&mut transform, buttons, dt);
            let new_pos: [f32; 3] = transform.translation.into();
            let displacement = Vec3::new(
                new_pos[0] - old_pos[0],
                new_pos[1] - old_pos[1],
                new_pos[2] - old_pos[2],
            );
            transform.translation = self.collision.move_and_slide(
                Vec3::from_array(old_pos),
                Vec3::from_array(PLAYER_HALF_EXTENTS),
                displacement,
                dt,
            );
        }

        if buttons.contains(InputButtons::FIRE) {
            self.fire_hitscan(entity, command.snapshot_ack_tick);
        }
    }

    /// Server-authoritative hitscan: a ray from the shooter along its facing,
    /// tested against every other player's AABB. The nearest hit's properties
    /// are run through the plugin's `on_hit` and merged back (game rule —
    /// non-predicted, ADR 0017). The facing comes from the player's look input
    /// (`Command.yaw/pitch`, applied in `on_command`).
    ///
    /// Targets are lag-compensated: validated against where the shooter's
    /// client saw them, not their current server positions (see
    /// `hit_candidates`).
    fn fire_hitscan(&mut self, shooter: EntityId, ack_tick: u64) {
        let Ok((origin, dir)) = self.simulation.transform_mut(shooter).map(|t| {
            (
                t.translation,
                (t.rotation * Vec3::NEG_Z).normalize_or_zero(),
            )
        }) else {
            return;
        };
        if dir == Vec3::ZERO {
            return;
        }

        let half = Vec3::from_array(PLAYER_HALF_EXTENTS);
        let candidates = self.hit_candidates(shooter, ack_tick);

        let Some(target) = blackflower_physics::hitscan::nearest_hit(origin, dir, half, candidates)
        else {
            return;
        };
        self.apply_hit(shooter, target);
    }

    /// Lag compensation: target centers rewound to the snapshot the shooter's
    /// client had acked (`ack_tick`), so hits are validated against the world
    /// the shooter actually saw. Falls back to current positions when that
    /// tick is no longer in the ring (missed or older than `RING_SIZE`). The
    /// shooter is excluded.
    fn hit_candidates(&self, shooter: EntityId, ack_tick: u64) -> Vec<(EntityId, Vec3)> {
        if let Some(snapshot) = self.ring.get(ack_tick) {
            return snapshot
                .entities
                .iter()
                .map(|e| (EntityId::from(e.id), Vec3::from_array(e.translation)))
                .filter(|(id, _)| *id != shooter)
                .collect();
        }
        self.simulation
            .targets()
            .into_iter()
            .filter(|(id, _)| *id != shooter)
            .map(|(id, t)| (id, t.translation))
            .collect()
    }

    /// Run the target's props through the plugin's `on_hit`. If the plugin
    /// declares the target dead, respawn it; otherwise merge the returned
    /// `(id, value)` pairs back into the target by id.
    fn apply_hit(&mut self, shooter: EntityId, target: EntityId) {
        let Ok(current) = self.simulation.props_mut(target).map(|p| p.0.clone()) else {
            return;
        };
        let Some(outcome) = self.plugin.as_mut().and_then(|p| p.on_hit(&current).ok()) else {
            return;
        };
        if outcome.respawn {
            self.respawn(target);
            debug!(%shooter, %target, "hitscan kill → respawn");
            return;
        }
        if let Ok(mut props) = self.simulation.props_mut(target) {
            for (id, value) in outcome.props {
                match props.0.iter_mut().find(|(pid, _)| *pid == id) {
                    Some(slot) => slot.1 = value,
                    None => props.0.push((id, value)),
                }
            }
        }
        debug!(%shooter, %target, "hitscan hit");
    }

    /// Reset a killed entity in place — fresh spawn transform and initial props,
    /// keeping the same `EntityId` so clients keep tracking it across the death.
    /// Death itself is a plugin rule (ADR 0017); the engine only resets state on
    /// the plugin's word.
    fn respawn(&mut self, target: EntityId) {
        let transform = self.next_spawn_transform();
        let props = self
            .plugin
            .as_mut()
            .and_then(|p| p.on_spawn().ok())
            .unwrap_or_default();
        if let Ok(mut t) = self.simulation.transform_mut(target) {
            *t = transform;
        }
        if let Ok(mut p) = self.simulation.props_mut(target) {
            p.0 = props;
        }
        info!(entity_id = %target, "respawned");
    }

    /// Transform for the next player spawn. The plugin (if any) picks which
    /// of the arena's spawn points to use — a game rule, server-authoritative
    /// and non-predicted (ADR 0017). Without a plugin we round-robin. The
    /// chosen spawn's `angle` becomes a yaw about the up axis.
    fn next_spawn_transform(&mut self) -> Transform {
        if self.spawn_points.is_empty() {
            return Transform::default();
        }
        let idx = self.select_spawn_index();
        let spawn = self.spawn_points[idx];
        Transform {
            translation: Vec3::from_array(spawn.origin),
            rotation: Quat::from_rotation_y(spawn.angle.to_radians()),
        }
    }

    /// Index into `spawn_points` for the next spawn — plugin choice, or
    /// round-robin fallback. Caller guarantees `spawn_points` is non-empty.
    fn select_spawn_index(&mut self) -> usize {
        let candidates: Vec<([f32; 3], f32)> = self
            .spawn_points
            .iter()
            .map(|s| (s.origin, s.angle))
            .collect();
        if let Some(idx) = self
            .plugin
            .as_mut()
            .and_then(|p| p.select_spawn(&candidates).ok())
        {
            return idx;
        }
        let idx = self.next_spawn % self.spawn_points.len();
        self.next_spawn = self.next_spawn.wrapping_add(1);
        idx
    }

    /// Reload the plugin if the watcher flagged its `.wasm` as changed, carrying
    /// internal state across via `save_state`/`load_state`. Any failure keeps the
    /// current plugin running so a bad build never drops the session.
    fn reload_plugin_if_changed(&mut self) {
        if !self.plugin_dirty.swap(false, Ordering::Relaxed) {
            return;
        }
        let Some(path) = self.plugin_path.clone() else {
            return;
        };
        let Some(old) = self.plugin.as_mut() else {
            return;
        };
        let state = match old.save_state() {
            Ok(state) => state,
            Err(error) => {
                warn!(%error, "plugin save_state failed; keeping current plugin");
                return;
            }
        };
        let mut next = match Plugin::load(&path) {
            Ok(plugin) => plugin,
            Err(error) => {
                warn!(%error, "plugin reload failed; keeping current plugin");
                return;
            }
        };
        if let Err(error) = next.load_state(&state) {
            warn!(%error, "plugin load_state failed; keeping current plugin");
            return;
        }
        self.plugin = Some(next);
        info!(path = %path.display(), "plugin hot-reloaded");
    }

    fn on_tick(&mut self, _tick: Tick, dt: f32) {
        blackflower_physics::systems::integrate_movement(
            self.simulation.query_mut::<(&mut Transform, &Velocity)>(),
            dt,
        );
    }
}

/// Watch the plugin's directory for changes to its `.wasm`, flagging `dirty`
/// when the file is written. Watches the parent dir (not the file) so it
/// survives the rename/replace `cargo build` does, and matches by file name to
/// sidestep path-canonicalization differences. Returns `None` (hot-reload
/// disabled, server still runs) if the watcher can't be set up.
fn spawn_plugin_watcher(path: &Path, dirty: Arc<AtomicBool>) -> Option<RecommendedWatcher> {
    let target = path.canonicalize().unwrap_or_else(|_| path.to_path_buf());
    let file_name = target.file_name()?.to_owned();
    let parent = target.parent()?.to_path_buf();

    let mut watcher = match recommended_watcher(move |res: notify::Result<NotifyEvent>| {
        let Ok(event) = res else {
            return;
        };
        let touched = matches!(event.kind, EventKind::Modify(_) | EventKind::Create(_));
        if touched
            && event
                .paths
                .iter()
                .any(|p| p.file_name() == Some(file_name.as_os_str()))
        {
            dirty.store(true, Ordering::Relaxed);
        }
    }) {
        Ok(watcher) => watcher,
        Err(error) => {
            warn!(%error, "plugin watcher unavailable; hot-reload disabled");
            return None;
        }
    };

    if let Err(error) = watcher.watch(&parent, RecursiveMode::NonRecursive) {
        warn!(%error, "watching plugin directory failed; hot-reload disabled");
        return None;
    }
    info!(path = %target.display(), "watching plugin for hot-reload");
    Some(watcher)
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

fn entity_full_delta(e: &EntitySnapshot) -> EntityDelta {
    EntityDelta {
        id: e.id,
        translation: Some(e.translation),
        rotation: Some(e.rotation),
        props: e.props.clone(),
        removed_props: vec![],
    }
}

fn entity_delta(curr: &EntitySnapshot, base: Option<&EntitySnapshot>) -> Option<EntityDelta> {
    let Some(base) = base else {
        return Some(entity_full_delta(curr));
    };
    let translation =
        field_changed(&curr.translation, &base.translation).then_some(curr.translation);
    let rotation = field_changed(&curr.rotation, &base.rotation).then_some(curr.rotation);
    let (changed_props, removed_props) = diff_props(&curr.props, &base.props);
    let has_changes = translation.is_some()
        || rotation.is_some()
        || !changed_props.is_empty()
        || !removed_props.is_empty();
    has_changes.then_some(EntityDelta {
        id: curr.id,
        translation,
        rotation,
        props: changed_props,
        removed_props,
    })
}

/// Returns `(changed, removed)` props by comparing current vs baseline.
/// Change detection is byte-exact — the engine does not interpret values.
fn diff_props(curr: &[Prop], base: &[Prop]) -> (Vec<Prop>, Vec<u16>) {
    let mut changed = Vec::new();
    let mut removed = Vec::new();

    for base_prop in base {
        if !curr.iter().any(|p| p.id == base_prop.id) {
            removed.push(base_prop.id);
        }
    }
    for curr_prop in curr {
        let baseline_val = base.iter().find(|p| p.id == curr_prop.id).map(|p| &p.value);
        let is_new_or_changed = baseline_val.is_none_or(|v| *v != curr_prop.value);
        if is_new_or_changed {
            changed.push(curr_prop.clone());
        }
    }

    (changed, removed)
}

/// Bit-exact change detection via `f32::to_bits`.
fn field_changed(a: &[f32], b: &[f32]) -> bool {
    a.iter()
        .zip(b.iter())
        .any(|(x, y)| x.to_bits() != y.to_bits())
}
