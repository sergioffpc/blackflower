# Blackflower

A Rust game engine for arena multiplayer shooters (up to 64 players @ 60 Hz),
with authoritative dedicated server and client-side prediction.

Quake 2-style architecture, modernized: archetype ECS (`hecs`), QUIC transport
(`quinn`), rollback-replay prediction, lock-free render pipeline.

## Status

Active development — M3 complete, M4 in progress. M3 delivered: slot state
machine, protocol handshake, delta snapshot compression, remote interpolation.
M4 (in progress): AABB arena geometry, WASM game-plugin architecture (engine
is agnostic to game properties — HP, damage, respawn are raw bytes owned by
the plugin), collision. See [`docs/ARCHITECTURE.md`](docs/ARCHITECTURE.md)
for the full design and milestone roadmap.

## Build

Requires Rust 1.95.0 (managed automatically by `rustup` via `rust-toolchain.toml`).

```bash
# Build engine
cargo build --workspace

# Build the arena-shooter WASM plugin (separate build, wasm32-wasip2 target)
rustup target add wasm32-wasip2
cargo build --manifest-path plugins/arena-shooter/Cargo.toml --target wasm32-wasip2

# Run the dedicated server with arena + plugin
cargo run --bin blackflowerd -- \
  --arena assets/arena.ron \
  --plugin plugins/arena-shooter/target/wasm32-wasip2/debug/arena_shooter.wasm

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
  blackflower-arena       Aabb, Arena (RON), collide_and_slide
  blackflower-authority   server tick loop, SlotState machine, delta broadcast
  blackflower-entity      stable EntityId (u64, 0 = NONE)
  blackflower-gameplay    pure simulation functions (shared by client + server)
  blackflower-graphics    wgpu renderer
  blackflower-input       InputButtons bitfield, InputHandle, Command generation
  blackflower-math        glam re-export + Transform component
  blackflower-network     QUIC transport (quinn), ServerHandle / ClientHandle
  blackflower-physics     Velocity component, integrate_movement system
  blackflower-plugin      wasmtime host — loads WASM game-plugin component
  blackflower-protocol    wire types: Command, WorldDelta, Prop, Request, Event
  blackflower-replica     client tick loop, prediction + reconciliation, ClockSync
  blackflower-tick        Tick counter, TickScheduler (configurable Hz)
  blackflower-world       SimulationWorld (server ECS), PresentationWorld (client)
  blackflower-audio       stub (kira wired, no logic yet)
assets/
  arena.ron         box arena geometry (50×4×50 m, 8 spawn points)
plugins/
  arena-shooter/    WASM game-plugin: HP, damage, respawn rules (wasm32-wasip2)
wit/
  game-plugin.wit   WIT contract between engine and game plugin
docs/
  ARCHITECTURE.md   design doc + ADRs
  diagrams/         SVG diagrams
```

## License

MIT — see [LICENSE](LICENSE).
