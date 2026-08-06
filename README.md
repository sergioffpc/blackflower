<p align="center">
  <img src="docs/assets/blackflower-mark.svg" alt="Blackflower logo" width="180">
</p>

<p align="center">
  <a href="https://github.com/sergioffpc/blackflower/actions/workflows/ci.yml"><img src="https://github.com/sergioffpc/blackflower/actions/workflows/ci.yml/badge.svg?branch=develop" alt="CI status"></a>
  <a href="https://github.com/sergioffpc/blackflower/actions/workflows/network-gate.yml"><img src="https://github.com/sergioffpc/blackflower/actions/workflows/network-gate.yml/badge.svg?branch=develop" alt="Network gate status"></a>
  <a href="LICENSE"><img src="https://img.shields.io/badge/license-MIT-blue.svg" alt="MIT license"></a>
  <a href="rust-toolchain.toml"><img src="https://img.shields.io/badge/rust-1.97.1-orange.svg?logo=rust" alt="Rust 1.97.1"></a>
</p>

Blackflower is an early-stage Rust workspace for a military multiplayer shooting
simulation. The project is organized around an authoritative server, a
deterministic fixed-step simulation, and a client that owns prediction and
presentation.

## Contents

- [Engine conventions](#engine-conventions)
- [Workspace](#workspace)
- [Running the applications](#running-the-applications)
- [Cooking assets](#cooking-assets)
- [Engineering principles](#engineering-principles)
- [Development setup](#development-setup)
- [Release builds](#release-builds)
- [Commit messages](#commit-messages)
- [Security](#security)
- [License](#license)

## Engine conventions

Blackflower engine space is right-handed: `+X` points right, `+Y` points up,
and `-Z` points forward. Linear distances use metres and angles use radians.
The normative basis, rotation, unit, matrix-layout, and foreign-format boundary
rules are recorded in the [coordinate-system contract](docs/coordinate-system.md).

## Workspace

| Path | Responsibility |
| --- | --- |
| `apps/blackflower` | Player client executable |
| `apps/blackflower-agent` | Headless ordinary-client agent shell and diagnostics executable |
| `apps/blackflower-server` | Authoritative server executable |
| `crates/blackflower-animation` | `.bfskel`/`.bfanim` runtime, evaluation, root motion, blending, and IK |
| `crates/blackflower-animation-format` | Native-free `.bfskel`/`.bfanim` format and rig identity |
| `crates/blackflower-acoustics` | Authoritative acoustic formats, Embree-backed propagation, and observations |
| `crates/blackflower-spatial-query` | Provider-neutral triangle scenes and bounded spatial queries backed by Embree |
| `crates/blackflower-assets` | Deterministic SquashFS asset packages and layered runtime VFS |
| `crates/blackflower-audio` | Public facade for the client audio stack |
| `crates/blackflower-audio-capture` | Lock-free CPAL microphone capture, voice worker, and server analysis |
| `crates/blackflower-audio-media` | 48 kHz PCM clips, lossless FLAC streams, sound-event formats, and offline cooking |
| `crates/blackflower-audio-playback` | Kira/CPAL mixing, HRTF tracks, and voice policy |
| `crates/blackflower-audio-spatial` | Statically linked Steam Audio spatial processing |
| `crates/blackflower-audio-voice` | Statically linked Opus voice encoding and decoding |
| `crates/blackflower-cooker-animation` | Host-only glTF-to-Ozz cooking and Blackflower container packaging |
| `crates/blackflower-cooker-acoustics` | Host-only Steam Audio presentation and pure-Rust authoritative acoustic cooking |
| `crates/blackflower-cooker-navigation` | Host-only Recast navmesh cooking |
| `crates/blackflower-cooker-vfx` | Host-only validation and cooking of authored visual effects |
| `crates/blackflower-cooker-volume` | Host-only OpenVDB-to-NanoVDB cooking |
| `crates/blackflower-destruction` | NVIDIA Blast asset, family, fracture, split, and stress bindings |
| `crates/blackflower-ecs` | Shared entity-component data and mechanisms |
| `crates/blackflower-gltf-metadata` | Versioned Blackflower authoring metadata in glTF and GLB |
| `crates/blackflower-harness` | Shared human/headless client session, replication, input, and prediction runtime |
| `crates/blackflower-navigation` | Detour navmesh loading, pathfinding, and runtime queries |
| `crates/blackflower-networking` | Shared networking primitives |
| `crates/blackflower-networking-quic` | Low-level Quinn transport endpoints and bounded host queues |
| `crates/blackflower-networking-replication` | Authoritative snapshot filtering, baselines, deltas, and quantization |
| `crates/blackflower-observability` | Process logging, metrics export, and profiler setup |
| `crates/blackflower-observability-tui` | Shared bounded logs and Prometheus polling for terminal observability dashboards |
| `crates/blackflower-rendering` | Immutable render-frame contract and latest-wins renderer handoff |
| `crates/blackflower-rendering-fluids` | NVIDIA Flow context bridge for renderer-owned GPU resources and passes |
| `crates/blackflower-rendering-models` | Validated runtime static meshes, generated LOD chains, and model hierarchies |
| `crates/blackflower-rendering-particles` | Presentation-only GPU particle simulation and rendering boundary |
| `crates/blackflower-rendering-textures` | KTX2 texture cooking and capability-driven runtime transcoding |
| `crates/blackflower-rendering-volumes` | VDB volume loading and CPU sampling |
| `crates/blackflower-shader-compiler` | Statically linked Slang-to-SPIR-V compiler binding |
| `crates/blackflower-scripting` | Backend-neutral policy scripting contracts |
| `crates/blackflower-scripting-luau` | Sandboxed Luau compilation and execution |
| `crates/blackflower-vfx` | Backend-neutral visual-effect assets and presentation cues |
| `crates/blackflower-world-prediction` | Client prediction and reconciliation |
| `crates/blackflower-world-presentation` | Client-only presentation systems |
| `crates/blackflower-world-simulation` | Authoritative fixed-step simulation |

## Running the applications

Run the player client after preparing the authenticated local fixture described
in the [client README](apps/blackflower/README.md):

```sh
cargo run --package blackflower --locked -- \
  --server-name localhost \
  --service-ca-certificate .local-network/service-ca.pem \
  --asset-package-directory target/assets/packages/debug \
  --asset-trust-key .local-network/asset-signing-public.pem
```

Run the authoritative server with the local certificates and asset trust key
created by `cargo xtask keys generate`:

```sh
RUST_LOG=info cargo run --package blackflower-server --locked -- \
  --listen-address 127.0.0.1:4433 \
  --tls-certificate .local-network/server-chain.pem \
  --tls-private-key .local-network/server-key.pem \
  --map-id maps/bootstrap \
  --asset-package-directory target/assets/packages/debug \
  --asset-trust-key .local-network/asset-signing-public.pem
```

Run the headless agent shell:

```sh
RUST_LOG=info cargo run --package blackflower-agent --locked
```

The shell composes the existing QUIC, shared client harness/prediction, and
Detour navigation boundaries for gameplay-owned configuration. It does not yet
install an observation encoder, policy, steering controller, or background
inference worker.

Run the server with the interactive foreground diagnostics dashboard:

```sh
cargo run --package blackflower-server --locked -- --foreground
```

The player client can keep its native window and presentation loop active while
showing client-side diagnostics in the launching terminal. Its panels are
Overview, Logs, Session, Prediction, Runtime/World, Presentation, and Host:

```sh
cargo run --package blackflower --locked -- \
  --foreground \
  --server-name localhost \
  --service-ca-certificate .local-network/service-ca.pem \
  --asset-package-directory target/assets/packages/debug \
  --asset-trust-key .local-network/asset-signing-public.pem
```

The agent has a separate foreground dashboard with Overview, Logs, Session,
Prediction, Navigation, and Host panels:

```sh
cargo run --package blackflower-agent --locked -- --foreground
```

Each dashboard reads the same Prometheus exposition as an external collector.
The default loopback ports are `9000` for the server, `9001` for an agent, and
`9002` for the player client. The server provides Overview, Logs, Simulation,
Transport, Sessions, Replication, World, and Host panels without requiring
Prometheus, Grafana, or a separate host-exporter process. Log capture and view
levels start at `INFO`, with no regex. Configure them directly on the Logs panel
with `L`, `l`, `/`, and `Escape`.

Use `1`-`8` or `Tab` to change server panels, `?` for the complete key map, and
`q` or `Ctrl-C` for orderly terminal restoration and shutdown. Unix `SIGTERM`
follows the same orderly shutdown path. See the
[observability policy](docs/observability.md#foreground-diagnostics) for the
data and isolation contract.

`blackflower-harness` is a reusable library rather than an executable. The
player frontend and headless bots use its same session, snapshot, input, and
prediction contract.

## Cooking assets

Validate cooking profiles, source/package manifests, and glTF/GLB sources:

```sh
cargo xtask assets check
```

Cook one deterministic runtime package:

```sh
cargo xtask assets cook \
    --profile release \
    --package pak000 \
    --signing-key /secure/asset-signing-key.pem
```

The profile name selects `assets/profiles/<name>.toml`. Cooking options are
defined once in that strict, versioned file and cannot be overridden by
individual assets. Profile schema 1 owns Luau optimization, debug, and
type-information settings, the portable SPIR-V compiler settings, and the KTX2
mipmap, and Zstandard policy. It also owns meshoptimizer LOD targets, error
limits, border locking, and overdraw optimization, plus Ozz sampling, iframe,
optimization, and root-motion tolerances. Luau coverage instrumentation is
always disabled. It fixes cooked audio at 48 kHz; recorded streams remain
lossless FLAC, while Opus is reserved for live voice. Acoustics runtime budgets and static Steam
Audio quality (rays, bounces, durations, pathing, and bake threads) are
centralized there too; per-asset
manifests own only authored probe placement. Model hierarchy and lossless
volume conversion have no profile settings. Each package embeds the profile
name and canonical
configuration hash.

The package name selects its only composition manifest:
`assets/source/packages/<logical-name>/package.toml`. Its explicit `assets`
list becomes the package contents; the cooker does not infer additional
members from handwritten dependencies. A selected animation clip pulls in its
typed skeleton dependency automatically; a selected model pulls in every Mesh
and Volume attachment; a selected sound event pulls in its referenced clip or
stream.

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

Artists can build the repository's Blender extension with
`python3 tools/blender/build_blackflower_gltf_metadata.py`. It writes
Action-local loop, additive, root-motion, Pose Marker, and typed model or level
node metadata directly to `extras.blackflower`; see the
[Blender metadata workflow](tools/blender/blackflower_gltf_metadata/README.md).
The same schema-1 exporter classifies acoustic geometry, identifies zone
volumes, portals, and probe volumes, and maps Blender materials to explicit
acoustic material IDs.

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

Compile the repository-global native vendors once for the active target and
Cargo profile before building crates that consume them:

```sh
cargo native build --profile debug
```

This produces every pinned native library below
`target/native/<target>/<configuration>/<crt>/`. Crate `build.rs` scripts only
compile their thin FFI wrappers and link the shared artifacts; they do not
rebuild vendor sources. Pass one or more vendor names after the options for a
focused build, for example `cargo native build --profile debug jolt opus`.
Use `--profile release` before a release Cargo build.
`BLACKFLOWER_NATIVE_DIR` can override the shared native root, while
`CARGO_TARGET_DIR` is honored automatically.

Native crates require a C/C++ compiler and libclang. The spatial and voice
audio crates also require CMake and link their native dependencies statically.
See the
[audio facade](crates/blackflower-audio/README.md),
[spatial audio setup](crates/blackflower-audio-spatial/README.md),
[voice audio setup](crates/blackflower-audio-voice/README.md),
[ECS setup](crates/blackflower-ecs/README.md),
[destruction setup](crates/blackflower-destruction/README.md),
[rendering fluids setup](crates/blackflower-rendering-fluids/README.md),
[rendering models format](crates/blackflower-rendering-models/README.md),
[rendering particles boundary](crates/blackflower-rendering-particles/README.md),
[rendering volumes setup](crates/blackflower-rendering-volumes/README.md),
[rendering textures setup](crates/blackflower-rendering-textures/README.md),
[shader compiler setup](crates/blackflower-shader-compiler/README.md),
[scripting contract](crates/blackflower-scripting/README.md), and
[Luau scripting setup](crates/blackflower-scripting-luau/README.md),
[VFX contract](crates/blackflower-vfx/README.md), and
[VFX cooker boundary](crates/blackflower-cooker-vfx/README.md) for details.
Steam Audio builds on supported x86-64 targets additionally require
the pinned ISPC compiler documented by the spatial audio setup.

IntelLLVM can be selected for native C/C++ dependencies by exporting `CC` and
`CXX` before invoking Cargo (`icx`/`icpx` on Linux, `icx-cl` with the Ninja
generator on Windows). Rust code is still compiled by the pinned Rust
toolchain and its LLVM backend. Intel does not provide its current oneAPI
C/C++ compiler for macOS or ARM64, so those targets use their platform
compiler.

## Release builds

Pushing a `v*` tag runs the release workflow and uploads six binary archives:

| Operating system | x86-64 native toolchain | ARM64 native toolchain |
| --- | --- | --- |
| Linux | IntelLLVM 2025.0.4 + ISPC 1.31.0 | Platform C/C++ compiler |
| Windows | IntelLLVM 2025.0.4 + ISPC 1.31.0 | MSVC |
| macOS | AppleClang + ISPC 1.31.0 | AppleClang |

Each archive contains `blackflower`, `blackflower-agent`, and
`blackflower-server` (with `.exe` suffixes on Windows). The x86-64 jobs also
build the statically linked Steam Audio crate, so the pinned ISPC,
Steam Audio, and Embree combination is part of the release gate. ARM64 Windows
uses GitHub's public-preview hosted runner and remains a required matrix entry.

Rust API documentation is generated and uploaded only by this tag workflow.
Pull-request CI continues to compile and test documentation examples, but does
not run `cargo doc` or publish a documentation artifact.

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
