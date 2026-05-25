# Blackflower

A Rust game engine for arena multiplayer shooters (up to 64 players @ 60Hz),
with authoritative dedicated server and client-side prediction.

Quake 2-style architecture, modernized: archetype-based ECS, QUIC transport,
hot-reloadable game module, deterministic asset pipeline.

## Status

Early development — milestone M0 (foundations).
See [`docs/architecture.md`](docs/architecture.md) for design and roadmap.

## Build

Requires Rust 1.95.0 (managed automatically by `rustup` via `rust-toolchain.toml`).

```bash
# Build all crates
cargo build --workspace

# Run the dedicated server
cargo run --bin blackflowerd

# Run the client
cargo run --bin blackflowerc

# Run linters
cargo fmt --all --check
cargo clippy --workspace --all-targets -- -D warnings
cargo test --workspace
```

## Layout

- `crates/blackflower-core/` — shared engine library (headless, deterministic).
- `bins/blackflowerd/` — dedicated server binary.
- `bins/blackflowerc/` — client binary.
- `docs/` — architecture document, ADRs, diagrams.

## License

MIT — see [LICENSE](LICENSE).
