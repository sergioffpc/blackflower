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
- `crates/blackflower-audio` — audio (stub, empty)
- `crates/blackflower-entity` — `EntityId` (stable 64-bit network-safe ID; 0 = NONE), `EntityIdAllocator`
- `crates/blackflower-gameplay` — shared simulation systems (e.g. `apply_player_movement`); pure functions run identically on client and server
- `crates/blackflower-graphics` — rendering: camera, geometry, pipelines, `Renderer` (wgpu/winit)
- `crates/blackflower-input` — `InputButtons` bitfield (used for client capture and server authority), `InputHandle`, produces `Command` per tick
- `crates/blackflower-math` — `glam` re-export + `Transform` component
- `crates/blackflower-network` — QUIC transport layer (quinn); `ClientHandle`, `ServerHandle`, wire codec
- `crates/blackflower-physics` — `Velocity` component, physics systems
- `crates/blackflower-prediction` — client-side prediction (stub)
- `crates/blackflower-protocol` — wire message types shared by client and server
- `crates/blackflower-tick` — `Tick` counter, `TickScheduler` (configurable Hz)
- `crates/blackflower-world` — `SimulationWorld` (server-side hecs ECS), `PresentationWorld` (client-side, applies snapshots)

### Simulation loop (blackflowerd)

`TickScheduler::new(tick_hz)` drives a fixed-rate loop (default 60 Hz via `--tick-rate-hz`). Each tick: run gameplay systems, generate a `Snapshot`, push it to all connected clients via `ServerHandle::try_send_snapshot()`. Overruns are logged as warnings.

### ECS (blackflower-world)

- `SimulationWorld` — server-side archetype ECS (backed by `hecs`). Spawns entities, runs systems, produces `Snapshot` values.
- `PresentationWorld` — client-side world that accepts and applies `Snapshot` values.
- `EntityId` — defined in `blackflower-entity`; stable 64-bit network-safe identifier; 0 is NONE.
- `Snapshot` = tick number + boxed slice of `EntitySnapshot` (id + translation + rotation). This is the entire replicated state sent over the wire each tick.

### Protocol types (blackflower-protocol)

- `Command { tick, buttons }` — client input sent as datagrams
- `Snapshot` / `EntitySnapshot` — server state broadcast as datagrams
- `Request` — framed stream messages client → server (currently `Hello`)
- `Event` — framed stream messages server → client (currently `Welcome { assigned_entity }`)

### Networking (blackflower-network)

QUIC transport via `quinn`. Wire encoding uses `postcard` (compact binary, `use-std` feature).

`ServerHandle<C, S, R, E>` and `ClientHandle<C, S, R, E>` are generic over four message types:
- `C` — Command (client→server datagram)
- `S` — Snapshot (server→client datagram)
- `R` — Request (client→server COBS-framed stream)
- `E` — Event (server→client COBS-framed stream)

**Wire codec (`blackflower_network`):**
- `encode` / `decode` — raw postcard, used for datagrams
- `encode_framed` / `decode_framed` — COBS-framed postcard (zero-terminated), used for streams; `decode_framed` returns `(message, consumed_bytes)`

**Server broadcast architecture (3 layers):**
1. Tick thread pushes `Snapshot` into a bounded crossbeam channel (capacity 8).
2. Dispatcher task wraps each snapshot in `Arc`, fans out to per-client tokio channels.
3. Per-client task encodes and sends as QUIC datagrams (capacity 3 ≈ 50 ms buffer; slow clients drop packets, others unaffected).

**Dev certs:** `cert.rs` generates self-signed certs and `SkipServerVerification` skips TLS verification. Not for production.

### Timing (blackflower-tick)

- `Tick` — newtype `u64`, monotonically increasing simulation step counter
- `TickScheduler::new(tick_hz)` — runtime-configurable rate; `dt_secs()` returns `1.0 / tick_hz`

### Key constraints from lint config

- **No `std::HashMap`/`HashSet`** — use `hashbrown` equivalents (enforced via `clippy.toml` disallowed types).
- **No `println!`/`dbg!`** — use `tracing` macros (enforced).
- **No `unwrap`/`expect`/`panic`** without a compelling reason (WARN level).
- **No `unsafe_code`** (DENY).
- Cognitive complexity ≤ 15, function body ≤ 80 lines, nesting ≤ 4 levels.
- Line width = 100, Unix LF, Rust 2024 edition.

### Toolchain

Pinned to Rust 1.95.0 via `rust-toolchain.toml`. Cross-compile targets included: `x86_64-unknown-linux-gnu`, `aarch64-unknown-linux-gnu` (for server deployments).
