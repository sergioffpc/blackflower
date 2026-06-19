# Blackflower

A Rust game engine for arena multiplayer shooters (up to 64 players @ 60 Hz),
with an authoritative dedicated server and client-side prediction.

Quake 2-style architecture, modernized: archetype ECS (`hecs`), QUIC transport
(`quinn`), rollback-replay prediction, lock-free render pipeline, and game rules
in a hot-reloadable WASM component (the engine is agnostic to game state — HP,
damage, and respawn are opaque bytes owned by the plugin).

## Status

Active development — **M4 complete, M5 in progress**.

- **M4 (done):** entity-based arenas, `rapier3d` collision, the WASM game-plugin
  architecture, and lag-compensated server-authoritative hitscan with
  plugin-driven death/respawn.
- **M5 (in progress):** mouse-look / first-person aim (done), plugin hot-reload
  with state migration (done); audio and a basic editor remain.

See [`docs/ARCHITECTURE.md`](docs/ARCHITECTURE.md) for the full design, ADRs, and
milestone roadmap.

## Build

Requires Rust 1.95.0 (managed automatically by `rustup` via `rust-toolchain.toml`).

```bash
# Build the engine
cargo build --workspace

# Build the e1m1 WASM game plugin (separate build, wasm32-wasip2 target)
rustup target add wasm32-wasip2
cargo build --manifest-path plugins/e1m1/Cargo.toml --target wasm32-wasip2

# Run the dedicated server with an arena (required) and the plugin (optional).
# Editing and rebuilding the .wasm hot-reloads the plugin without a restart.
cargo run --bin blackflowerd -- \
  --arena-path assets/maps/e1m1.ron \
  --plugin-path plugins/e1m1/target/wasm32-wasip2/debug/e1m1.wasm

# Run the client (connects to 127.0.0.1:3512; reads assets/blackflowerc.toml)
cargo run --bin blackflowerc

# Simulate network conditions for prediction testing (both binaries accept these)
cargo run --bin blackflowerd -- --fake-latency-ms 80 --fake-jitter-ms 20
cargo run --bin blackflowerc -- --fake-latency-ms 40 --fake-jitter-ms 10

# Lint and test
cargo fmt --all --check
cargo clippy --workspace --all-targets -- -D warnings
cargo test --workspace
```

The server is configured entirely by CLI flags (`--arena-path` is required;
`--plugin-path`, `--tick-hz`, `--max-clients`, `--bind-addr` are optional). The
client reads a TOML config (`--config-path`, default `assets/blackflowerc.toml`)
for the window size, mouse-look sensitivity, and key bindings.

## Layout

```
bins/
  blackflowerd/   dedicated server
  blackflowerc/   game client (winit window + wgpu renderer)
crates/
  blackflower-audio       stub (kira wired, no logic yet)
  blackflower-authority   server tick loop, SlotState machine, delta broadcast,
                          hitscan + lag comp, plugin hot-reload (authority.rs + ring.rs)
  blackflower-gameplay    pure simulation (movement, look) shared by client + server,
                          plus the wasmtime WASM-plugin host (plugin.rs)
  blackflower-graphics    wgpu renderer + first-person camera
  blackflower-input       InputButtons bitfield, mouse-look (yaw/pitch), Command generation
  blackflower-math        glam re-export + Transform component
  blackflower-network     QUIC transport (quinn), ServerHandle / ClientHandle, wire codec
  blackflower-physics     rapier3d collision + ray-vs-AABB hitscan (server-only)
  blackflower-protocol    wire types: Command, WorldDelta, Properties, Request, Event
  blackflower-replica     client tick loop, prediction + reconciliation, ClockSync
  blackflower-time        Tick counter, TickScheduler (configurable Hz)
  blackflower-window      winit window + event loop (keyboard + mouse)
  blackflower-world       SimulationWorld (server ECS), PresentationWorld (client),
                          shared Entities wrapper, EntityId, arena/map loading
assets/
  maps/e1m1.ron      entity-based arena (solids + spawn points)
  blackflowerc.toml  client config (window, look sensitivity, key bindings)
plugins/
  e1m1/             WASM game plugin: HP, damage, respawn, spawn selection (wasm32-wasip2)
wit/
  game-plugin.wit   WIT contract between engine and game plugin
docs/
  ARCHITECTURE.md   design doc + embedded ADRs (inline ASCII diagrams)
```

## License

MIT — see [LICENSE](LICENSE).
