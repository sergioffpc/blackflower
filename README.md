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
| `crates/blackflower-assets` | Deterministic SquashFS asset packages and layered runtime VFS |
| `crates/blackflower-audio` | Pure Rust facade for the client audio stack |
| `crates/blackflower-audio-spatial` | Statically linked Steam Audio spatial processing |
| `crates/blackflower-audio-voice` | Statically linked Opus voice encoding and decoding |
| `crates/blackflower-ecs` | Shared entity-component data and mechanisms |
| `crates/blackflower-navigation` | Detour navmesh loading, pathfinding, and runtime queries |
| `crates/blackflower-networking` | Shared networking primitives |
| `crates/blackflower-observability` | Process logging, metrics export, and profiler setup |
| `crates/blackflower-simulation` | Authoritative fixed-step simulation |
| `crates/blackflower-prediction` | Client prediction and reconciliation |
| `crates/blackflower-presentation` | Client-only presentation systems |
| `crates/blackflower-rendering-volumes` | VDB volume loading and CPU sampling |
| `crates/blackflower-scripting` | Sandboxed Luau compilation and execution |

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

## Cooking assets

Validate source and package manifests:

```sh
cargo xtask assets check
```

Cook one deterministic runtime package:

```sh
cargo xtask assets cook \
    --profile desktop-universal \
    --package pak000 \
    --signing-key /secure/asset-signing-key.pem
```

The package name selects its only composition manifest:
`assets/source/packages/<logical-name>/package.toml`. Its explicit `assets`
list becomes the package contents; the cooker does not infer additional
members from handwritten dependencies.

Runtime package directories use Quake-style lexical overrides. A package such
as `pak900-hotfix.squashfs` overrides matching asset IDs from
`pak000.squashfs`; unrelated assets continue to resolve from lower packages.
Every package carries an Ed25519 signature over its BLAKE3 SquashFS payload
digest. The executable must supply the permitted public keys; private keys are
offline cooker inputs and must never be committed.
Development runtimes may enable the opt-in `blackflower-assets/hot-reload`
feature. It provides transactional snapshot reloads and a debounced native
watcher for successful recooks; production builds leave it disabled.
See the [asset VFS documentation](crates/blackflower-assets/README.md) for the
portable naming and identity contracts.

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

Native crates require a C/C++ compiler and libclang. The spatial and voice
audio crates also require CMake and compile their native dependencies
statically from pinned submodules. See the
[audio facade](crates/blackflower-audio/README.md),
[spatial audio setup](crates/blackflower-audio-spatial/README.md),
[voice audio setup](crates/blackflower-audio-voice/README.md),
[ECS setup](crates/blackflower-ecs/README.md),
[rendering volumes setup](crates/blackflower-rendering-volumes/README.md), and
[scripting setup](crates/blackflower-scripting/README.md) for details.

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
