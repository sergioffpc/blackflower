# Proposed MVP repository layout

Implemented subset: [#21](https://github.com/sergioffpc/blackflower/issues/21)
supplies the
[uv-managed OpenUSD cooker, signed primitive packs and C++ content loader](content-pipeline.md).
The rest of the runtime/SDK design below remains proposed. The cooker produces
`.bfserver`, `.bfagent` and `.bfclient` files. Applications select paths; the
loader validates resource schemas without a role parameter.

Status: directory and build-target proposal. The owner requires exactly two
product runtimes, client and server in C++23, and one offline cooker in Python.
Existing source/build files have not been moved or reconfigured by this
document.

## Source layout

```text
blackflower/
├── CMakeLists.txt
├── CMakePresets.json
├── vcpkg.json
├── src/
│   ├── client/
│   │   ├── CMakeLists.txt
│   │   ├── main.cc
│   │   ├── application/              # Startup, scheduling, external effects
│   │   ├── worlds/
│   │   │   ├── prediction/
│   │   │   └── presentation/
│   │   └── adapters/
│   │       ├── input/                # Human and autonomous input
│   │       ├── graphics/             # Falcor / cooked SPIR-V loading
│   │       └── audio/                # Steam Audio and output backend
│   ├── server/
│   │   ├── CMakeLists.txt
│   │   ├── main.cc
│   │   ├── application/              # Startup, admission orchestration, ticks
│   │   └── worlds/
│   │       └── simulation/
│   └── modules/
│       ├── contracts/                # C++ value types and module interfaces
│       ├── simulation/               # Shared movement rules; no I/O
│       ├── content/                  # Pack verification and runtime loading
│       ├── networking/               # Protocol encoding and GNS adapter
│       └── physics/                  # Separate CPU/GPU PhysX adapter targets
├── tools/
│   └── cooker/
│       ├── pyproject.toml
│       ├── src/
│       │   └── content/
│       │       ├── __init__.py
│       │       ├── __main__.py
│       │       ├── cli.py
│       │       ├── pipeline.py
│       │       ├── processors/
│       │       │   ├── model_import.py       # Assimp integration
│       │       │   ├── mesh_optimization.py  # meshoptimizer integration
│       │       │   ├── model_encoding.py    # Final runtime mesh format
│       │       │   └── shader_compilation.py # Linux Slang to SPIR-V
│       │       ├── pack.py           # Canonical pack/manifest writer
│       │       └── signing.py        # Established crypto/tool integration
│       └── tests/                   # Python cooker tests
├── schemas/
│   ├── pack/                        # Binary layout, canonical encoding, versions
│   ├── scene/                       # Source and cooked scene formats
│   └── protocol/                    # Network representation once selected
├── assets/
│   ├── scenes/
│   ├── meshes/
│   ├── materials/
│   ├── textures/
│   ├── shaders/
│   └── audio/
├── tests/
│   ├── client/
│   ├── server/
│   ├── modules/
│   ├── integration/
│   └── fixtures/
│       └── packs/                   # Small shared format/signature test vectors
├── benchmarks/
├── cmake/                           # Existing toolchains and build support
├── docs/
└── build/                           # Ignored generated files
```

Keep repository conventions, automation, and root documents (`AGENTS.md`,
`CONTEXT.md`, licensing, `.agents`, `.github`, and related files) in their
existing locations. Create only directories with an implemented responsibility;
this tree is not a request for empty scaffolding or speculative module
interfaces. Each C++ module owns a CMake target and a narrow published header
surface; internal headers remain private to the target.

## Responsibilities and dependency rules

The runtime `main.cc` files are thin entry points. Each `application/` assembles
adapters, verifies the pack, creates worlds, and drives external operations
between ECS execution segments. World implementations contain the accepted
phases and in-memory state only. SDK calls stay in adapters, following
[ADR-0004](adr/0004-keep-io-outside-ecs.md).

`src/modules/` contains named capabilities used by both C++ runtimes, not a
general-purpose `common` or `utils` collection. In particular:

-   `contracts` defines in-memory identities, commands, snapshots, and physics
    request/results without Flecs or SDK handles. These C++ types do not define
    the wire format by memory layout.
-   `simulation` contains shared input/movement rules used by the authoritative
    and predicted worlds. It does not own another world or invoke physics
    devices.
-   `content`, `networking`, and `physics` are application-owned integration
    modules. Worlds consume their value contracts, not their external
    operations. Build separate CPU/GPU physics targets from the relevant
    sources; the Linux server must not inherit client GPU/render/audio
    dependencies.

Client and server never link to each other's application or world
implementation. Both can link the named shared modules as appropriate. World
targets link Flecs plus allowed data/rule modules; application targets select
the SDK adapters. Private link/include boundaries and review must enforce this
distinction because directory names alone cannot prevent I/O.

The owner-selected cooker stack is Python, Assimp for model import,
meshoptimizer for optimization before final-format conversion, and Slang for
offline compilation to SPIR-V. The proposed processor files keep native
import/optimization interfaces and final encoding separate; the shader processor
invokes Slang for the required SPIR-V output. Add audio processing as its
delivery needs arise.

The Python cooker is a separately installable command-line package with
`pyproject.toml` and a `src/` import layout. This layout helps tests exercise
the installed package instead of accidentally importing files from the working
directory; see the
[Python Packaging User Guide](https://packaging.python.org/en/latest/discussions/src-layout-vs-flat-layout/).
The implemented cooker pins Python 3.14, uses uv with uv.lock and setuptools,
and delegates signing through cryptography or a caller-supplied signer. See
[setup and checks](content-pipeline.md). Installing Python is a
cooker/development concern, not an application-level requirement added to the
server or client by this tool; any existing SDK-owned runtime dependencies are
evaluated separately.

Assimp/meshoptimizer bindings and native libraries are cooker dependencies built
or installed for the Linux host, not the Windows cross-compilation target. Pin
and validate the Python/native combinations together. Offline Python processors
can invoke pinned Linux-host SDK tools, including Slang, or use a proven native
binding where required. Availability of a PhysX cooker binding is not assumed.
Any necessary native helper is a private build-time implementation detail of the
cooker, not another deployed product runtime; its need and exact route remain a
feasibility question.

## Shared formats and source assets

`schemas/` is the language-neutral authority for serialized data: field
meanings, byte order, versioning, canonical signing bytes, and bounds. It does
not imply JSON as the runtime pack format or a selected schema generator. Python
and C++ implementations consume that specification and common vectors under
`tests/fixtures/packs/`; they need not share parser source code.

Conformance checks must have Python produce a signed pack that the C++ loader
accepts, and have both implementations agree on canonical bytes, payload hashes,
and content build identity. Each staged runtime receives the files it needs and
its independently provisioned trust set. Production signing private keys never
enter this tree. Disposable test material must be clearly identified and cannot
become the runtime trust set.

`assets/` contains authoring inputs. Cooked artifacts go under `build/packs/`,
with only small test vectors committed as fixtures. Runtime staging contains the
signed pack and required runtime files, not the authoring tree or cooker
environment. Large source assets may need a separate storage policy when they
actually exist.

## Targets and generated outputs

| Product                      | Source root               | Proposed entry point / artifact                                                                                                                                                  |
| ---------------------------- | ------------------------- | -------------------------------------------------------------------------------------------------------------------------------------------------------------------------------- |
| Windows client, C++23        | `src/client/`             | CMake target `blackflower_client`, installed executable `blackflower-client.exe`.                                                                                                |
| Linux server, C++23          | `src/server/`             | CMake target `blackflower_server`, installed executable `blackflower-server`.                                                                                                    |
| Offline Linux cooker, Python | `tools/content_pipeline/` | Installed CLI `cooker`, also runnable as an installed module `python -m cooker`. A Python package distribution is sufficient; no frozen executable is required by this proposal. |

Test and benchmark executables remain development artifacts. Shared libraries or
SDK helper tools do not introduce additional Blackflower product runtimes.

Retain the existing CMake preset output roots, such as `build/debug`,
`build/release`, and `build/windows-release`. Propose explicit `bin/` output
locations for the new runtime targets and separate staging directories:

```text
build/
├── release/bin/blackflower-server
├── windows-release/bin/blackflower-client.exe
├── cooker/
│   ├── venv/                        # Isolated Python environment
│   ├── cache/                       # Intermediate cooked resources
│   └── dist/                        # Python package artifacts
├── packs/
│   ├── mvp.bfserver
│   ├── mvp.bfagent
│   └── mvp.bfclient
└── stage/
    ├── server-linux-x64/
    │   ├── bin/blackflower-server
    │   ├── content/mvp.bfserver
    │   └── licenses/
    └── client-windows-x64/
        ├── bin/blackflower-client.exe
        ├── content/mvp.bfclient
        └── licenses/
```

These are intended output paths, not paths produced by today's build. Required
shared runtime libraries belong in each staged package at platform-appropriate
loader locations. CMake configures the Linux server and cross-compiled Windows
client separately; Python tooling is independently provisioned. A future
packaging command can orchestrate all three build/cook steps without making
either game runtime execute Python.

## Migration from the current bootstrap

The current `src/main.cc` and root CMake target `blackflower` are a console
bootstrap. During the first implementation slice, replace that target with
explicit client/server entry points, move target configuration into their
directories, and preserve the existing analysis, signing-hook, sanitizer, test,
and benchmark workflows. Update source collection for clang-tidy/format checks
so files in new modules are covered. Add the Python cooker and cross-language
pack conformance checks alongside the first content delivery.

The content module, primitive cooker, schemas and fixtures are implemented; the
broader directory proposal does not request speculative scaffolding. The
[architecture](architecture.md) and [cooker design](cooker-and-packs.md)
distinguish the selected languages from this directory proposal.
