# CLAUDE.md

This file provides guidance to Claude Code (claude.ai/code) when working with code in this repository.

## Commands

```bash
# Build
cargo build --workspace

# Run server (listens on 0.0.0.0:3512; --arena-path is required)
cargo run --bin blackflowerd -- --arena-path assets/maps/e1m1.ron

# Run client (connects to 127.0.0.1:3512; reads assets/blackflowerc.toml)
cargo run --bin blackflowerc

# Simulate network conditions for testing prediction (both binaries accept these flags)
cargo run --bin blackflowerd -- --fake-latency-ms 80 --fake-jitter-ms 20
cargo run --bin blackflowerc -- --fake-latency-ms 40 --fake-jitter-ms 10

# Server options (defaults shown; --arena-path is required)
cargo run --bin blackflowerd -- --arena-path assets/maps/e1m1.ron --tick-hz 60 --max-clients 64 --bind-addr 0.0.0.0:3512

# Build the e1m1 WASM plugin (requires wasm32-wasip2 target)
rustup target add wasm32-wasip2
cargo build --manifest-path plugins/e1m1/Cargo.toml --target wasm32-wasip2

# Server arena and plugin are CLI flags (no config file).
# --arena-path (required) → arena/map RON; --plugin-path (optional) → WASM component.
cargo run --bin blackflowerd -- --arena-path assets/maps/e1m1.ron --plugin-path plugins/e1m1/target/wasm32-wasip2/debug/e1m1.wasm

# Client config is still TOML (`[bindings]` key → action); override with --config-path.
cargo run --bin blackflowerc -- --config-path assets/blackflowerc.toml

# Format check
cargo fmt --all --check

# Lint (all warnings are errors)
cargo clippy --workspace --all-targets -- -D warnings

# Tests
cargo test --workspace

# Single test
cargo test --package <crate-name> <test_name>
```

## Architecture

Blackflower is a Rust game engine for arena multiplayer shooters (up to 64 players @ 60 Hz) using an authoritative dedicated server with client-side prediction, inspired by Quake 2.

### Workspace layout

- `bins/blackflowerd` — dedicated game server binary
- `bins/blackflowerc` — game client binary (winit window + wgpu renderer)
- `crates/blackflower-audio` — `AudioEngine`: kira spatial audio. A listener tracks the local player's camera; each `GameEvent` plays a one-shot on a transient per-event spatial track (panned/attenuated by position). Sounds are loaded **by id** (file stem) from `assets/sounds/*.wav` — `play(sound_id, position)`; adding a sound is dropping a file in (no recompile). Init/dir-load failures are non-fatal (runs silent).
- `crates/blackflower-gameplay` — pure simulation functions (e.g. `apply_player_movement`); run identically on client and server. `plugin` module hosts the WASM Component Model plugin (`Plugin`, wasmtime host)
- `crates/blackflower-graphics` — rendering: camera, geometry, pipelines, `Renderer` (wgpu/winit)
- `crates/blackflower-input` — `InputButtons` bitfield, `InputHandle`, produces `Command` per tick
- `crates/blackflower-math` — `glam` re-export + `Transform { translation: Vec3, rotation: Quat }`
- `crates/blackflower-network` — QUIC transport layer (quinn); `ClientHandle`, `ServerHandle`, wire codec
- `crates/blackflower-physics` — `Velocity` component, `integrate_movement` system; `collision` module (`CollisionWorld`: rapier3d static cuboid colliders + kinematic character controller for server-side player move-and-slide). Server-only — not in the client dependency graph.
- `crates/blackflower-authority` — server-side authority loop, `SlotState` machine (`Handshake → Playing → Zombie`), delta snapshot broadcast
- `crates/blackflower-replica` — client tick loop, `PredictionState` (rollback-replay), `ClockSync` (NTP clock estimation), `SnapshotAck` (sliding-window ack bitfield)
- `crates/blackflower-protocol` — wire message types shared by client and server
- `crates/blackflower-time` — `Tick` counter, `TickScheduler` (configurable Hz)
- `crates/blackflower-world` — `SimulationWorld` (server-side hecs ECS), `PresentationWorld` (client-side, applies snapshots), `EntityId`/`EntityIdAllocator` (stable 64-bit network-safe ID; 0 = NONE), `arena` module (entity-based map: `Arena { id, entities }` from `assets/maps/*.ron`, `MapEntity { classname, props }`; derives solid `Aabb`s and spawn points by classname — collision itself lives in `blackflower-physics`)

### Configuration

Each binary owns its `Args` (clap) and `App` in its own `app.rs` module; `main.rs` is a thin entrypoint that parses args and calls `app::run_app`.

- **Server** — all CLI flags, no config file. `--arena-path` (required) = arena/map RON file, loaded via `Arena::load`; `--plugin-path` (optional, omit to run without a plugin) = WASM component. Other knobs: `--tick-hz`, `--max-clients`, `--bind-addr`, `--fake-latency-ms`, `--fake-jitter-ms`.
- **Client** — loads a TOML config via `--config-path` (default `assets/blackflowerc.toml`), parsed with the `toml` crate into a serde struct. `[window]` table sets `width`/`height` (each defaults to 1280×720; the table may be omitted). `[look]` table sets `sensitivity` (radians of view rotation per pixel of mouse motion; defaults to 0.0022; table may be omitted). `[bindings]` table maps a physical key name (as emitted by `blackflower-window`, e.g. `"W"`) to an action — an `InputButtons` flag name resolved case-insensitively via `InputButtons::from_action` (`forward`/`backward`/`left`/`right`/`fire`). Many keys may map to the same action. Unknown action names fail at startup. The resolved `HashMap<String, InputButtons>` lives in `App` and drives `on_key_down`/`on_key_up`; `on_mouse_motion` feeds mouse-look. Remaining CLI flags: `--config-path`, `--server-addr`, `--fake-latency-ms`, `--fake-jitter-ms`.

### Server simulation loop (blackflowerd)

`TickScheduler::start(tick_hz, cb)` drives a fixed-rate loop (default 60 Hz via `--tick-hz`). Each tick:
1. Drain connects → insert `SlotState::Handshake`
2. Drain `Request::Hello` → version + capacity check; `Handshake → Playing`, send `Event::Welcome`; or send `Event::Rejected`
3. Drain `Request::Ping` → send `Event::Pong` (NTP clock sync)
4. Drain `Command` datagrams per `Playing` client → apply `apply_player_movement` then `CollisionWorld::move_and_slide` (server-authoritative wall collision), record `last_processed`, advance `baseline_tick` from ack bitfield
5. Drain disconnects → `Playing → Zombie` (entity held 5 s), `Handshake → removed`
6. Expire zombies → despawn entity, remove slot
7. Run `integrate_movement` for all `(Transform, Velocity)` entities
8. Build `WorldSnapshot`, insert into `SnapshotRing` (32 entries)
9. Per `Playing` client: `build_delta` against confirmed baseline → send `WorldDelta`

Two distinct acks: `WorldDelta.ack` (server→client, highest command tick processed, for prediction reconciliation) and `Command.snapshot_ack_tick/bits` (client→server, sliding-window bitfield of received snapshots, for baseline selection).

### Client threading model (blackflowerc)

Three threads, no mutexes on the hot path:

- **Main thread** — winit event loop: `App::on_draw()` reads the framebuffer, `on_key_down/up()` mutates `InputHandle`
- **Tick thread** — spawned once; runs `TickScheduler` at 60 Hz; owns `PresentationWorld`, `PredictionState`, `ClientHandle`
- **Network thread** — hidden inside `ClientHandle::connect()` (tokio runtime); feeds channels the tick thread polls

The tick thread publishes render-ready state via `Arc<ArcSwap<Box<[(EntityId, Transform)]>>>`. The main thread calls `framebuffer.load()` — a lock-free atomic load. Neither thread ever blocks the other.

### Client-side prediction (blackflower-replica)

`PredictionState` keeps a ring buffer of 128 `HistoryEntry { tick, buttons, transform }` (~2 s at 60 Hz). Each tick:

1. **Predict** — apply `apply_player_movement` locally with the captured buttons, push to history
2. **Reconcile** (when a new snapshot arrives with an `ack`) — discard history entries ≤ ack, roll back to the server's authoritative transform, then replay remaining unacked inputs in order
3. **Extract** — overwrite the local player's position in the world with the predicted transform before publishing to the framebuffer

Gameplay functions must remain pure (no side effects, no RNG) so predict and server runs are identical given the same inputs.

### ECS (blackflower-world)

- `SimulationWorld` — server-side. Wraps `hecs::World`. Spawns entities with arbitrary components, produces `WorldSnapshot` for the ring buffer.
- `PresentationWorld` — client-side. Applies `WorldDelta` (full or incremental via `apply_delta`); `extract()` returns classified entities (`Predicted` for local player, `Interpolated` with sample history for remotes).
- `EntityId` — monotonically allocated from 1; 0 is `NONE` (sentinel). IDs are never reused so stale commands can't target a replacement entity.

### Protocol types (blackflower-protocol)

- `Command { tick, buttons, yaw, pitch, snapshot_ack_tick, snapshot_ack_bits }` — client input sent as unreliable datagrams; `yaw`/`pitch` are absolute view angles (radians, sent absolute not as deltas); ack fields carry a 32-bit sliding-window of received snapshot ticks
- `WorldDelta { tick, ack, baseline, removed, entities: Box<[EntityDelta]>, events: Vec<GameEvent> }` — server→client datagram; `baseline == 0` = full snapshot, `baseline > 0` = delta against that tick; `ack` is the highest client command tick processed (for prediction reconciliation); `events` are transient per-tick `GameEvent { kind: GameEventKind, position }` published by the plugin — `GameEventKind` is an extensible enum of client-facing categories (`Sound(id)` today; animations/particles later), payloads opaque to the engine (lossy — a dropped delta drops its events)
- `EntityDelta { id, translation: Option<[f32;3]>, rotation: Option<[f32;4]> }` — only changed fields are `Some`; change detection via `f32::to_bits()`
- `WorldSnapshot / EntitySnapshot` — full entity state, used internally by server's `SnapshotRing` (not sent on wire directly)
- `PROTOCOL_VERSION: u32` — checked during handshake
- `Request` — COBS-framed stream messages client → server: `Hello { protocol_version }`, `Ping { client_send_ns }`
- `Event` — COBS-framed stream messages server → client: `Welcome { tick_hz, assigned_entity_id }`, `Rejected { reason: RejectReason }`, `Pong { client_send_ns, server_tick }`

### Networking (blackflower-network)

QUIC transport via `quinn`. Wire encoding uses `postcard` (compact binary).

`ServerHandle<C, S, R, E>` and `ClientHandle<C, S, R, E>` are generic over four message types:
- `C` — Command (client→server datagram)
- `S` — Snapshot (server→client datagram)
- `R` — Request (client→server COBS-framed stream)
- `E` — Event (server→client COBS-framed stream)

**Wire codec:**
- `encode` / `decode` — raw postcard, for datagrams
- `encode_framed` / `decode_framed` — COBS-framed postcard (zero-terminated), for streams; returns `(message, consumed_bytes)`

**Server broadcast (3 layers):**
1. Tick thread pushes `WorldDelta` into a bounded crossbeam channel (capacity 8).
2. Dispatcher task wraps each delta in `Arc`, fans out to per-client tokio channels.
3. Per-client task encodes and sends as QUIC datagrams (capacity 3 ≈ 50 ms buffer; slow clients drop, others unaffected).

Deltas are intentionally lossy — an older delta is worthless once a newer one arrives. A missed delta forces the next one to be a full snapshot (baseline not in ring).

**Dev certs:** `cert.rs` generates self-signed certs; `SkipServerVerification` skips TLS verification. Not for production.

### Timing (blackflower-time)

- `Tick` — newtype `u64`, monotonically increasing simulation step counter
- `TickScheduler::start(tick_hz, cb)` — runtime-configurable rate; `dt_secs()` returns `1.0 / tick_hz`; logs overruns as warnings

### Key constraints from lint config

- **No `std::HashMap`/`HashSet`** — use `hashbrown` equivalents (enforced via `clippy.toml`).
- **No `println!`/`dbg!`/`eprint!`** — use `tracing` macros (enforced).
- **No `todo!`** — use explicit `unimplemented!` or open an issue (enforced).
- **No `unsafe_code`** (DENY).
- **No `unwrap`/`expect`/`panic`** without a compelling reason (WARN level).
- Cognitive complexity ≤ 15, function body ≤ 100 lines, max arguments ≤ 6, nesting ≤ 5 levels.
- Max 2 `bool` fields per struct, max 2 `bool` function parameters (use enums or newtypes beyond that).
- Max 3 single-character bindings per scope.
- Type sizes: pass-by-value ≤ 128 bytes, enum variant ≤ 128 bytes, error type ≤ 64 bytes.
- Line width = 100, Unix LF, Rust 2024 edition.

### Toolchain

Pinned to Rust 1.95.0 via `rust-toolchain.toml`. Cross-compile targets included: `x86_64-unknown-linux-gnu`, `aarch64-unknown-linux-gnu` (for server deployments).

## Current state (2026-06-19)

**Milestone:** M4 **closed** (Phases A + B + C). M5 in progress — aim/look input + plugin hot-reload delivered.

**M4-A delivered:**
- `blackflower-world::arena` — entity-based map (`Arena { id, entities }` / `MapEntity { classname, props }` from `assets/maps/*.ron`); solids and spawn points derived by classname. Map entities use classnames (`solid_brush`, `spawn_point`) with opaque string `props` (`"x y z"`); the engine interprets only solids/spawns.
- Collision via `blackflower-physics::collision::CollisionWorld` (rapier3d kinematic character controller), server-authoritative (not predicted — see ADR 0017).
- WASM Component Model plugin: `wit/game-plugin.wit`, `blackflower-gameplay::plugin` (wasmtime 45 host), `plugins/e1m1` (wasm32-wasip2 guest)
- Engine-agnostic properties: `Prop { id: u16, value: Vec<u8> }` — raw bytes, engine never interprets
- Players spawn at arena spawn points chosen by the plugin (`select-spawn` returns an index into the map's candidates; engine falls back to round-robin with no plugin), collide with walls via rapier

**M4-A refactor:** the standalone `blackflower-arena`, `blackflower-plugin`, and `blackflower-entity` crates were folded into existing crates — arena geometry into `blackflower-world::arena`, the WASM host into `blackflower-gameplay::plugin`, and `EntityId`/`EntityIdAllocator` into `blackflower-world`.

**M4-B delivered (weapon + hitscan):**
- `InputButtons::FIRE` bit (`1 << 4`); client binds it via `"fire"` (e.g. `Space` in `blackflowerc.toml`).
- `blackflower-physics::hitscan::ray_aabb` — pure slab-method ray-vs-AABB (unit-tested).
- `blackflower-authority::fire_hitscan` — when `FIRE` is set, casts a ray from the shooter along its facing (`rotation * -Z`, now driven by mouse-look — see M5 below), tests against every other player's AABB (`±PLAYER_HALF_EXTENTS` via `SimulationWorld::targets`), nearest hit only.
- On hit: `plugin.on_hit(target_props)` → merge returned `(id, value)` props back into the target's `EntityProps` by id (`SimulationWorld::props_mut`).
- Server-authoritative, non-predicted (ADR 0017).
- **Known MVP gaps:** aim reuses the spawn facing (no look/aim input yet) — *resolved in M5, see below*; fires once per tick while `FIRE` is held (no edge-detection / fire-rate).

**M4-C delivered (lag compensation + respawn):**
- Lag comp: `Authority::hit_candidates` rewinds target positions to the snapshot at `command.snapshot_ack_tick` (via `SnapshotRing::get`); falls back to current positions when that tick has aged out of the ring. `fire_hitscan` now takes the ack tick.
- Death is a plugin rule (engine is opaque to HP): WIT `on-hit` returns `hit-result { props, respawn }`. Host exposes `HitOutcome { props, respawn }`.
- On `respawn`: `Authority::respawn` resets the target's transform (`next_spawn_transform`) + props (`on_spawn`), same `EntityId`. Otherwise props merge by id as before.
- e1m1 guest sets `respawn` when HP reaches 0.
- `SnapshotRing` (insert/get/eviction, keyed by `Tick`) + `highest_acked` live in `blackflower-authority` (`ring.rs` after the module split). No unit tests.

**M5 in progress — aim/look input delivered:**
- Mouse-look → absolute view angles on the wire: `Command` gains `yaw`/`pitch` (radians, absolute). (`PROTOCOL_VERSION` left at 1 — pre-release, client and server build together.)
- `blackflower-input::InputHandle` holds `{ buttons, yaw, pitch }`; `look(dyaw, dpitch)` accumulates (pitch clamped to ±~89°). Client app `on_mouse_motion` scales raw `DeviceEvent::MouseMotion` by `[look] sensitivity`.
- `blackflower-window`: `WindowHandler::on_mouse_motion`, focus-gated `device_event`, best-effort cursor grab (`Locked`→`Confined`) + hide on focus.
- Pure rule `blackflower-gameplay::systems::apply_player_look` (rotation = yaw·pitch); `apply_player_movement` is now **yaw-relative** (forward/right derived from facing). Applied look-before-move on both server (`Authority::on_command`) and client prediction (`PredictionState` predict+reconcile carry yaw/pitch).
- First-person client: `Replica::state` returns `RenderState { camera, entities }` — local player drives the camera (`Renderer::render(camera_from, ..)`, `Camera::look_along`) and is not drawn; remotes interpolated as before.
- Hitscan facing now follows the look direction (what you see is what you shoot; ray origin = camera eye = translation).
- Unit tests in `blackflower-gameplay`: look facing, yaw-relative forward, pitch doesn't lift movement.

**M5 — plugin hot-reload + state migration delivered:**
- WIT gains `save-state -> list<u8>` / `load-state(list<u8>)` (opaque bytes; plugin owns versioning/migration). Host: `Plugin::save_state`/`load_state`.
- `blackflower-authority` watches the plugin `.wasm` via `notify` (parent dir, matched by file name) → sets an `AtomicBool`; the tick thread's `reload_plugin_if_changed` (top of `do_tick`) reloads: `save_state(old)` → `Plugin::load(new)` → `load_state` → swap. Any failure logs and keeps the current plugin (session never drops).
- `Authority::start` now takes `Option<PathBuf>` (loads the plugin itself + sets up the watcher); `blackflowerd` no longer loads it.
- e1m1 guest serializes `NEXT_SPAWN` as `[version][u64 LE]`; entity props (HP) live in the engine and survive reloads regardless.

**M5 — spatial audio delivered:**
- **Plugin-published events (event bus):** the WIT world gains an *import* `play-sound(sound: string, position: vec3)` the guest calls, plus `on-fire(shooter-pos)` and `on-hit(props, target-pos)` (positions passed in). The plugin publishes directly to the bus — `PluginState.events` (the wasmtime Store data the import handler writes to). The engine **drains the bus once per tick** in `broadcast_snapshots` (`Plugin::drain_events`), maps `PlaySound`→`GameEvent { kind: GameEventKind::Sound(id), position }`, and attaches it to every client's `WorldDelta`. No engine-side fire/hit/death semantics, no per-hook event plumbing: `on-hit` returns only the engine-actionable `HitOutcome` (props/respawn); sounds go to the bus. `GameEventKind` is extensible (Sound today; animations/particles are future variants — add a `play-*` import + variant). e1m1 publishes `"fire"`/`"hit"`/`"death"`.
- `blackflower-audio::AudioEngine` (kira): one listener follows the local player's camera; each event plays a one-shot on a transient spatial track (`persist_until_sounds_finish`), panned/attenuated by world position. Sounds loaded by id from `assets/sounds/*.wav`. glam's `mint` feature gives the kira interop — `Vec3`/`Quat` pass straight to kira. The replica tick thread owns the engine, plays `snapshot.events`, updates the listener from the predicted local transform.
- Non-fatal: no audio device or missing WAVs → runs silent. Sounds expected at `assets/sounds/{fire,hit,death}.wav` (user supplies the files).

**M5 remaining:** basic editor; fire-rate / edge-detect on `FIRE` (deferred by user).
