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
- `bins/blackflowerc` — game client binary
- `crates/blackflower-core` — headless shared simulation library (no UI/rendering deps)
- `crates/blackflower-net` — QUIC transport layer
- `crates/blackflower-render` — rendering (wgpu/winit, currently empty)

### Simulation loop (blackflowerd)

The server runs a fixed 60 Hz tick loop: call `integrate_movement()`, generate a `Snapshot`, send it to all connected clients via `ServerHandle::send_snapshot()`. Overruns are logged as warnings.

### ECS (blackflower-core/src/ecs)

- `SimulationWorld` — server-side archetype ECS (backed by `hecs`). Spawns entities, runs systems, produces `Snapshot` values.
- `PresentationWorld` — client-side world that accepts and applies `Snapshot` values.
- `EntityId` — stable 64-bit network-safe identifier; 0 is reserved as NONE.
- Components (`Transform`, `Velocity`) are `#[repr(C)]` + `Copy` for bulk ECS iteration.
- `Snapshot` = tick number + Vec of `EntitySnapshot` (id + Transform). This is the entire replicated state sent over the wire each tick.

### Networking (blackflower-net)

QUIC transport via `quinn`. Wire encoding uses `postcard` (compact binary).

**Message types:** `ClientToServer::Subscribe` / `ServerToClient::Snapshot(Snapshot)`

**Server broadcast architecture (3 layers):**
1. Tick thread pushes `Snapshot` into a bounded crossbeam channel (capacity 8).
2. Dispatcher task drains channel, wraps in `Arc`, fans out to all connected clients.
3. Per-client task encodes and sends snapshots as QUIC datagrams (capacity 3 ≈ 50 ms buffer; slow clients drop packets).

**Client:** single background thread with a tokio runtime. Opens a bidi stream, sends `Subscribe`, receives datagrams. `ClientHandle::drain_snapshots()` is the pull API for the main loop.

**Dev certs:** `cert.rs` generates self-signed certs and `SkipServerVerification` skips TLS verification. Not for production.

### Timing constants (blackflower-core/src/time.rs)

- `TICK_HZ = 60` — simulation rate
- `TICK_DURATION = 16_667 µs` — wall-clock interval
- `TICK_DT_SECS = 1.0/60.0` — delta-time passed to systems

### Key constraints from lint config

- **No `std::HashMap`/`HashSet`** — use `hashbrown` equivalents (enforced via `clippy.toml` disallowed types).
- **No `println!`/`dbg!`** — use `tracing` macros (enforced).
- **No `unwrap`/`expect`/`panic`** without a compelling reason (WARN level).
- **No `unsafe_code`** (DENY).
- Cognitive complexity ≤ 15, function body ≤ 80 lines, nesting ≤ 4 levels.
- Line width = 100, Unix LF, Rust 2024 edition.

### Toolchain

Pinned to Rust 1.95.0 via `rust-toolchain.toml`. Cross-compile targets included: `x86_64-unknown-linux-gnu`, `aarch64-unknown-linux-gnu` (for server deployments).
