# Blackflower

Blackflower is an early-stage Rust workspace for a military multiplayer shooting
simulation. The project is organized around an authoritative server, a
deterministic fixed-step simulation, and a client that owns prediction and
presentation.

## Workspace

| Path | Responsibility |
| --- | --- |
| `apps/blackflower` | Player client executable |
| `apps/blackflower-server` | Authoritative server executable |
| `apps/blackflower-harness` | Simulation and integration test harness |
| `crates/blackflower-audio` | Statically linked Steam Audio HRTF processing |
| `crates/blackflower-ecs` | Shared entity-component data and mechanisms |
| `crates/blackflower-navigation` | Detour navmesh loading, pathfinding, and runtime queries |
| `crates/blackflower-observability` | Process logging, metrics export, and profiler setup |
| `crates/blackflower-simulation` | Authoritative fixed-step simulation |
| `crates/blackflower-prediction` | Client prediction and reconciliation |
| `crates/blackflower-presentation` | Client-only presentation systems |

## Running the applications

Run the player client:

```sh
RUST_LOG=info cargo run --package blackflower --locked
```

Run the authoritative server:

```sh
RUST_LOG=info cargo run --package blackflower-server --locked
```

Run the simulation and integration harness:

```sh
RUST_LOG=info cargo run --package blackflower-harness --locked
```

## Engineering principles

- The server validates commands and owns authoritative state.
- Simulation behavior must be deterministic and replayable.
- Client prediction must converge to authoritative state.
- Protocol and persisted data formats must use explicit versions and byte order.
- Runtime failures must be propagated or handled instead of aborting the process.
- Rendering, audio, and other presentation concerns must not mutate authoritative
  simulation state.

## Development setup

Install [rustup](https://rustup.rs/). The repository pins Rust `1.97.1` with the
minimal toolchain profile and installs the `rustfmt` and `clippy` components
automatically.

Clone with the vendored native dependencies:

```sh
git clone --recurse-submodules https://github.com/sergioffpc/blackflower.git
```

If the repository is already cloned, initialize them with:

```sh
git submodule update --init --recursive
```

Native crates require a C/C++ compiler and libclang. The audio crate also
requires CMake and compiles Steam Audio, PFFFT, libmysofa and zlib statically
from pinned submodules. See the
[audio setup](crates/blackflower-audio/README.md) and
[ECS setup](crates/blackflower-ecs/README.md) for details.

Enable the versioned Git hooks after cloning:

```sh
./scripts/setup-git-hooks.sh
```

Run the same quality checks used by the pre-push hook and CI:

```sh
./scripts/ci.sh
```

See the [observability policy](docs/observability.md) for logging, tracing,
metrics, profiling, and deterministic diagnostic boundaries.

## Commit messages

Commits use a single-line Conventional Commits message:

```text
feat(simulation): add fixed-step scheduler
```

See [CONTRIBUTING.md](CONTRIBUTING.md) for the complete contribution policy.

## Security

Do not report suspected vulnerabilities in a public issue. Follow the private
reporting process in [SECURITY.md](SECURITY.md).

## License

Blackflower is licensed under the [MIT License](LICENSE).
