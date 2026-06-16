# CLAUDE.md

This file provides guidance to Claude Code (claude.ai/code) when working with code in this repository.

## Commands

```bash
# Build
cargo build --workspace

# Run server (listens on 0.0.0.0:3512)
cargo run --bin blackflowerd

# Run client (connects to 127.0.0.1:3512)
cargo run --bin blackflowerc

# Simulate network conditions for testing prediction (both binaries accept these flags)
cargo run --bin blackflowerd -- --fake-latency-ms 80 --fake-jitter-ms 20
cargo run --bin blackflowerc -- --fake-latency-ms 40 --fake-jitter-ms 10

# Server options (defaults shown)
cargo run --bin blackflowerd -- --tick-hz 60 --max-clients 64 --bind-addr 0.0.0.0:3512

# Build the arena-shooter WASM plugin (requires wasm32-wasip2 target)
rustup target add wasm32-wasip2
cargo build --manifest-path plugins/arena-shooter/Cargo.toml --target wasm32-wasip2

# Run server with arena + plugin
cargo run --bin blackflowerd -- \
  --arena assets/arena.ron \
  --plugin plugins/arena-shooter/target/wasm32-wasip2/debug/arena_shooter.wasm

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
- `crates/blackflower-audio` — audio stub (kira dependency wired, no logic yet)
- `crates/blackflower-entity` — `EntityId` (stable 64-bit network-safe ID; 0 = NONE), `EntityIdAllocator`
- `crates/blackflower-gameplay` — pure simulation functions (e.g. `apply_player_movement`); run identically on client and server
- `crates/blackflower-graphics` — rendering: camera, geometry, pipelines, `Renderer` (wgpu/winit)
- `crates/blackflower-input` — `InputButtons` bitfield, `InputHandle`, produces `Command` per tick
- `crates/blackflower-math` — `glam` re-export + `Transform { translation: Vec3, rotation: Quat }`
- `crates/blackflower-network` — QUIC transport layer (quinn); `ClientHandle`, `ServerHandle`, wire codec
- `crates/blackflower-physics` — `Velocity` component, `integrate_movement` system
- `crates/blackflower-authority` — server-side authority loop, `SlotState` machine (`Handshake → Playing → Zombie`), delta snapshot broadcast
- `crates/blackflower-replica` — client tick loop, `PredictionState` (rollback-replay), `ClockSync` (NTP clock estimation), `SnapshotAck` (sliding-window ack bitfield)
- `crates/blackflower-protocol` — wire message types shared by client and server
- `crates/blackflower-tick` — `Tick` counter, `TickScheduler` (configurable Hz)
- `crates/blackflower-world` — `SimulationWorld` (server-side hecs ECS), `PresentationWorld` (client-side, applies snapshots)

### Server simulation loop (blackflowerd)

`TickScheduler::start(tick_hz, cb)` drives a fixed-rate loop (default 60 Hz via `--tick-hz`). Each tick:
1. Drain connects → insert `SlotState::Handshake`
2. Drain `Request::Hello` → version + capacity check; `Handshake → Playing`, send `Event::Welcome`; or send `Event::Rejected`
3. Drain `Request::Ping` → send `Event::Pong` (NTP clock sync)
4. Drain `Command` datagrams per `Playing` client → apply `apply_player_movement`, record `last_processed`, advance `baseline_tick` from ack bitfield
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

- `Command { tick, buttons, snapshot_ack_tick, snapshot_ack_bits }` — client input sent as unreliable datagrams; ack fields carry a 32-bit sliding-window of received snapshot ticks
- `WorldDelta { tick, ack, baseline, removed, entities: Box<[EntityDelta]> }` — server→client datagram; `baseline == 0` = full snapshot, `baseline > 0` = delta against that tick; `ack` is the highest client command tick processed (for prediction reconciliation)
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

### Timing (blackflower-tick)

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
