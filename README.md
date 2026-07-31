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
| `crates/blackflower-animation` | `.bfskel`/`.bfanim` runtime, evaluation, root motion, blending, and IK |
| `crates/blackflower-animation-format` | Native-free `.bfskel`/`.bfanim` format and rig identity |
| `crates/blackflower-acoustics` | Pure-Rust authoritative acoustic formats, BVH, propagation, and observations |
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
| `crates/blackflower-cooker-volume` | Host-only OpenVDB-to-NanoVDB cooking |
| `crates/blackflower-ecs` | Shared entity-component data and mechanisms |
| `crates/blackflower-gltf-metadata` | Versioned Blackflower authoring metadata in glTF and GLB |
| `crates/blackflower-navigation` | Detour navmesh loading, pathfinding, and runtime queries |
| `crates/blackflower-networking` | Shared networking primitives |
| `crates/blackflower-networking-replication` | Authoritative snapshot filtering, baselines, deltas, and quantization |
| `crates/blackflower-observability` | Process logging, metrics export, and profiler setup |
| `crates/blackflower-rendering-models` | Validated runtime static meshes, generated LOD chains, and model hierarchies |
| `crates/blackflower-rendering-textures` | KTX2 texture cooking and capability-driven runtime transcoding |
| `crates/blackflower-rendering-volumes` | VDB volume loading and CPU sampling |
| `crates/blackflower-shader-compiler` | Statically linked Slang-to-SPIR-V compiler binding |
| `crates/blackflower-scripting` | Sandboxed Luau compilation and execution |
| `crates/blackflower-world-prediction` | Client prediction and reconciliation |
| `crates/blackflower-world-presentation` | Client-only presentation systems |
| `crates/blackflower-world-simulation` | Authoritative fixed-step simulation |

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

Native crates require a C/C++ compiler and libclang. The spatial and voice
audio crates also require CMake and compile their native dependencies
statically from pinned submodules. See the
[audio facade](crates/blackflower-audio/README.md),
[spatial audio setup](crates/blackflower-audio-spatial/README.md),
[voice audio setup](crates/blackflower-audio-voice/README.md),
[ECS setup](crates/blackflower-ecs/README.md),
[rendering models format](crates/blackflower-rendering-models/README.md),
[rendering volumes setup](crates/blackflower-rendering-volumes/README.md),
[rendering textures setup](crates/blackflower-rendering-textures/README.md),
[shader compiler setup](crates/blackflower-shader-compiler/README.md), and
[scripting setup](crates/blackflower-scripting/README.md) for details.
Steam Audio builds on supported x86-64 targets additionally require
the pinned ISPC compiler documented by the spatial audio setup.

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
