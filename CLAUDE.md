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
- `crates/blackflower-authority` — server-side authority loop, session management (`conn_entities`, `last_processed`)
- `crates/blackflower-replica` — client tick loop, `PredictionState` (rollback-replay), `ClockSync` (NTP clock estimation)
- `crates/blackflower-protocol` — wire message types shared by client and server
- `crates/blackflower-tick` — `Tick` counter, `TickScheduler` (configurable Hz)
- `crates/blackflower-world` — `SimulationWorld` (server-side hecs ECS), `PresentationWorld` (client-side, applies snapshots)

### Server simulation loop (blackflowerd)

`TickScheduler::start(tick_hz, cb)` drives a fixed-rate loop (default 60 Hz via `--tick-rate-hz`). Each tick:
1. Drain pending `Request::Hello` messages → assign `EntityId`, send `Event::Welcome { assigned_entity }`
2. Drain pending `Command` datagrams per client → apply `apply_player_movement`, record `last_processed[client]`
3. Drain disconnects → despawn entity
4. Run `integrate_movement` for all `(Transform, Velocity)` entities
5. For each client: build `Snapshot { tick, ack: last_processed[client], entities }` and send as datagram

The `ack` field echoes the highest client tick the server has processed — clients use this for prediction reconciliation.

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

- `SimulationWorld` — server-side. Wraps `hecs::World`. Spawns entities with arbitrary components, produces `Snapshot` values.
- `PresentationWorld` — client-side. Upserts entities from snapshots; `extract()` returns a flat `Vec<(EntityId, Transform)>` for the renderer.
- `EntityId` — monotonically allocated from 1; 0 is `NONE` (sentinel). IDs are never reused so stale commands can't target a replacement entity.

### Protocol types (blackflower-protocol)

- `Command { tick, buttons }` — client input sent as unreliable datagrams
- `Snapshot { tick, ack, entities: Box<[EntitySnapshot]> }` — server state broadcast as unreliable datagrams; `ack` is the highest client tick the server processed
- `Request` — COBS-framed stream messages client → server (currently `Hello`)
- `Event` — COBS-framed stream messages server → client (currently `Welcome { assigned_entity }`)

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
1. Tick thread pushes `Snapshot` into a bounded crossbeam channel (capacity 8).
2. Dispatcher task wraps each snapshot in `Arc`, fans out to per-client tokio channels.
3. Per-client task encodes and sends as QUIC datagrams (capacity 3 ≈ 50 ms buffer; slow clients drop, others unaffected).

Snapshots are intentionally lossy — an older snapshot is worthless once a newer one arrives.

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
