# Blackflower

A Rust game engine for arena multiplayer shooters (up to 64 players @ 60 Hz),
with authoritative dedicated server and client-side prediction.

Quake 2-style architecture, modernized: archetype ECS (`hecs`), QUIC transport
(`quinn`), rollback-replay prediction, lock-free render pipeline.

## Status

Active development — M3 complete. Implemented: authoritative server, QUIC
networking, ECS, client-side prediction with rollback-replay, slot state
machine (Handshake → Playing → Zombie), protocol version handshake, delta
snapshot compression with per-client ack bitfield, and remote entity
interpolation. See [`docs/architecture.md`](docs/architecture.md) for the
full design and milestone roadmap.

## Build

Requires Rust 1.95.0 (managed automatically by `rustup` via `rust-toolchain.toml`).

```bash
cargo build --workspace

# Run the dedicated server (listens on 0.0.0.0:3512)
cargo run --bin blackflowerd

# Run the client (connects to 127.0.0.1:3512)
cargo run --bin blackflowerc

# Simulate network conditions for prediction testing
cargo run --bin blackflowerd -- --fake-latency-ms 80 --fake-jitter-ms 20
cargo run --bin blackflowerc -- --fake-latency-ms 40 --fake-jitter-ms 10

# Lint and test
cargo fmt --all --check
cargo clippy --workspace --all-targets -- -D warnings
cargo test --workspace
```

## Layout

```
bins/
  blackflowerd/   dedicated server
  blackflowerc/   game client (winit + wgpu)
crates/
  blackflower-authority   server tick loop, SlotState machine, delta broadcast
  blackflower-entity      stable EntityId (u64, 0 = NONE)
  blackflower-gameplay    pure simulation functions (shared by client + server)
  blackflower-graphics    wgpu renderer
  blackflower-input       InputButtons bitfield, InputHandle, Command generation
  blackflower-math        glam re-export + Transform component
  blackflower-network     QUIC transport (quinn), ServerHandle / ClientHandle
  blackflower-physics     Velocity component, integrate_movement system
  blackflower-protocol    wire types: Command, WorldDelta, Request, Event
  blackflower-replica     client tick loop, prediction + reconciliation, ClockSync
  blackflower-tick        Tick counter, TickScheduler (configurable Hz)
  blackflower-world       SimulationWorld (server ECS), PresentationWorld (client)
  blackflower-audio       stub (kira wired, no logic yet)
docs/
  architecture.md   design doc + ADRs
  diagrams/         SVG diagrams
```

## License

MIT — see [LICENSE](LICENSE).
