# Blackflower architecture

Status: initial implementation baseline with agreed MVP requirements. This
document follows the [arc42 structure](https://arc42.org/overview/). Open items
identify missing information, not agreed requirements or implemented behavior.

## 1. Introduction and goals

Blackflower is intended to be a first-person simulation platform for training in
virtual scenarios, with high physical, acoustic, and visual realism,
representing situations and environments that are difficult to reproduce in the
physical world. Each session will feature competition between an attacking Red
Team and a defending Blue Team, with up to eight participants per team.

The repository contains a console bootstrap, build/test infrastructure, and an
[OpenUSD-to-signed-pack pipeline](content-pipeline.md) with a C++ consumer
harness and headless Flecs scene instantiation. Simulation behavior has not been
implemented.

The
[first MVP specification](https://github.com/sergioffpc/blackflower/issues/11)
narrows the initial delivery to up to four players in one fixed LAN scenario,
with first-person movement, blocking collisions, simple visuals, and virtual
hits producing a sound for the two involved players. Team rules and training
evaluation are outside this MVP; the broader project description above remains
the product direction.

The repository owner has requested arc42 adoption, English documentation, and
signed Conventional Commits. Product stakeholders and their expectations remain
to be identified.

Open beyond this technical MVP: identify representative training users, specific
learning objectives, operational use cases, and measurable realism requirements
before expanding the solution.

## 2. Architecture constraints

The repository is public on GitHub. Blackflower uses the
[MIT License](../LICENSE); third-party materials retain their original notices.
Contribution constraints are maintained in [AGENTS.md](../AGENTS.md).

Development capacity is one or two people. The two product runtimes, client and
server, use C++23 with Clang 21 under the
[C++ engineering guidelines](cpp-guidelines.md); the offline cooker uses Python
with uv and OpenUSD scene input, with Assimp for model import and meshoptimizer
for optimization before final-format conversion, as selected by the owner. Build
tooling uses CMake, Ninja, and sccache; vcpkg manages dependencies. Visual
Studio Code is the [reference editor](editor.md). The build targets
x64-linux-clang and cross-compiles x64-windows-clang from Linux; GitHub CI runs
only the Linux target on Ubuntu 26.04. See the [build guide](build.md) for
toolchain and platform prerequisites.

The owner requires an authoritative Linux server and Windows clients
cross-compiled in Linux for the MVP. Its reference deployment is specified in
section 7. The owner selected Falcor, PhysX, Slang, Flecs,
GameNetworkingSockets, and Steam Audio; the
[technology stack](technology-stack.md) records their proposed placement and
compatibility work. The owner also requires offline-cooked content delivered in
three verified signed packs per scenario from the MVP onward, with Slang
compiling client shaders to SPIR-V offline; see the
[cooker specification](https://github.com/sergioffpc/blackflower/issues/19) and
[design](cooker-and-packs.md). Vulkan is the only allowed graphics backend;
DirectX 12 is excluded by [ADR-0006](adr/0006-use-vulkan-only.md). A Falcor
exception adapter and the audio device backend remain proposals. Deployment and
fidelity requirements for the broader training platform remain open.

Use the standard library first, with Boost as the preferred source for missing
capabilities under the
[library selection policy](cpp-guidelines.md#library-selection). GoogleTest and
Google Benchmark provide the requested test and measurement frameworks through
vcpkg. Static analysis uses clang-tidy under the
[analysis policy](static-analysis.md). Concrete Boost components remain to be
selected when required.

Project-owned Python follows the
[Google Python Style Guide and enforcement policy](python-guidelines.md). Pyink
formats Python; Pylint checks conventions and static diagnostics; mypy checks
types. Native CTest and CI include these checks alongside the C++ validation
workflow.

The [shared style guidelines](style-guidelines.md) are the entry point for C++,
Python, JavaScript, Markdown, JSON, and shell conventions. Prettier,
markdownlint, ESLint with Google, Stylistic, JSDoc and JSON rules, ShellCheck,
shfmt, and actionlint enforce the documented source checks in the Linux Debug CI
job, including vendored skills and extensionless Git hooks. Consumer schemas and
the semantic review requirements remain authoritative.

## 3. Context and scope

For the intended MVP, an operator starts the Linux server and up to four players
connect Windows clients on a local network. Players supply movement, look, and
firing input; clients present the shared scene and relevant hit sounds. The
scenario is prepared during development. There are no required accounts, lobby,
neighboring services, or scenario-authoring interfaces.

The [MVP specification](https://github.com/sergioffpc/blackflower/issues/11)
defines the boundary and acceptance behavior. Specific training roles, learning
outcomes, and neighboring systems for the broader product remain undefined.

The [C4 system context](c4.md#c1-system-context) depicts the operator, players,
and intended software system.

## 4. Solution strategy

The owner selected a server-authoritative shared simulation: the Linux server
validates actions and maintains the official game state, while Windows clients
provide input and presentation. The
[selected technology stack](technology-stack.md) now supplies rendering,
physics, ECS, networking, shaders, and audio processing. The
[C4 proposal](c4.md) places CPU PhysX and the Simulation World on the server,
and two separate Flecs worlds in each client: a Prediction World and a
Presentation World, with Falcor consuming cooked SPIR-V and audio serving
presentation. Human and autonomous-agent sources supply the same command
interface. The owner requires GPU PhysX client prediction against static
geometry only; dynamic state and dynamic collision response come from the
authoritative server. The exact GPU movement integration remains to be proven.
GameNetworkingSockets connects the executables over the LAN.

The selected technologies, two client worlds, interchangeable input, GPU client
physics, and static-only prediction are owner requirements. Detailed integration
remains proposed and unimplemented;
[ADR-0002](adr/0002-separate-client-worlds-and-input.md) records the separation
and its consequences. The detailed module interfaces, wire schema, scheduling,
and synchronization must be validated against the
[MVP specification](https://github.com/sergioffpc/blackflower/issues/11). The
[rendering/physics](research/nvidia-rendering-physics-compatibility.md) and
[Flecs/Valve](research/valve-flecs-compatibility.md) research records concrete
integration constraints, including Falcor's host/target build assumptions and
error policy.

For each major approach, explain which goal it serves and link to the relevant
decision in section 9. Treat unvalidated choices as proposals.

## 5. Building block view

| Building block                                                     | Responsibility                                                                                                                                   |
| ------------------------------------------------------------------ | ------------------------------------------------------------------------------------------------------------------------------------------------ |
| [Console bootstrap](../src/main.cc)                                | Print the project name and return a startup status.                                                                                              |
| [GoogleTest harness](../tests/build_test.cc)                       | Exercise test integration and the C++23 build contract.                                                                                          |
| [Google Benchmark harness](../benchmarks/framework_benchmark.cc)   | Exercise benchmark registration and execution.                                                                                                   |
| [Offline cooker](../tools/content_pipeline/src/cooker/pipeline.py) | Snapshot bounded OpenUSD references, encode entities and owned bounds, sign and verify all three packs, then publish their directory atomically. |
| [Content module](../src/modules/content/content.h)                 | Own pack bytes and authenticate their manifest and payload before returning validated scene values.                                              |
| [Content harness](../tests/content_harness.cc)                     | Consume a pack using independent public-key trust; expose IDs and dimensions for cross-language integration checks.                              |
| [Build configuration](../CMakeLists.txt)                           | Build four executables and content/scene libraries; run analysis, Python checks and integration tests.                                           |

The [runtime scene module](runtime-scenes.md) publishes authenticated immutable
SceneAssets and shared collider resources, then instantiates recipes directly in
a minimal Flecs world. SceneInstance retains source ownership and membership;
LocalTransform and collider references live only in ECS. Public synchronous
operations compose root placement, update, transfer, destroy and unload, with
generation-checked entity/resource identities. Collider definitions serve
Simulation and Prediction; shared spatial definitions never imply shared mutable
world state. This headless foundation does not implement world progression.

The content loader accepts packs independently of consumer purpose or host
platform. The shared content build identity relates the three artifacts; there
is no separate artifact identifier. The authenticated file magic selects the
ServerScene, AgentScene or ClientScene alternative of the returned variant.
Resource schemas define representation compatibility. It separates layout
decoding, authentication and resource validation internally while retaining one
public loading contract. A scene contains immutable prototypes with authored
string identity, placement and optional local colliders, independent of visual
assets. Decoding checks identity order, finite transforms, positive dimensions,
unit rotations and complete byte records before exposing content. Geometry and
gameplay suitability belong to consumers. An absent `Entities` or entity
`Bounds` scope produces the corresponding empty collection.

The [USD authoring contract](usd-authoring.md) implements referenced entities
under a Scene root, with explicit Cube bounds and preserved oriented boxes.
Scenarios define exercises and objectives separately. GLB visuals and textured
content remain pending under #23 and #59.

Local authoring inputs are selected through the cooker's `--source` argument;
the repository has no dedicated local asset directory. The reference scene lives
in `tests/integration/fixtures/mvp.usda`, following the
[source asset layout](repository-layout.md#shared-formats-and-source-assets).

GoogleTest and Google Benchmark are linked only into their respective harnesses.
Flecs 4.1.6 supplies headless ECS storage, and owner-selected GLM 1.0.3 supplies
quaternion composition; the pinned vcpkg baseline fixes both dependencies.
[Sanitizer configuration](../cmake/Sanitizers.cmake) instruments non-Release
project targets for memory checks, with a separate Linux configuration for race
detection. There are no simulation modules yet.

As implementation begins, document the main modules, their responsibilities,
interfaces, and dependencies, with links to the source. Add detail where it
helps explain important boundaries.

The proposed [C4 container view](c4.md#c2-containers),
[server components](c4.md#c3-authoritative-server-components), and
[client components](c4.md#c3-windows-client-components) describe the next
software structure. Runtime libraries run inside the client and server
executable types. The [offline cooker](cooker-and-packs.md) produces the two
scenario packs; [runtime application orchestration](runtime-lifecycle.md) owns
verification, resource preparation, and world startup/shutdown. The
[repository layout proposal](repository-layout.md) separates C++ runtime
sources, shared modules, the Python cooker, language-neutral formats, and
generated artifacts. The Simulation World interface and client
prediction/reconciliation interface are the principal behavior-test surfaces.
Input-source equivalence and presentation isolation are checked through those
interfaces; running-client/server and perceptual checks cover the actual
integrations.

## 6. Runtime view

On startup, the bootstrap writes the project name to standard output and returns
success unless the write reports failure. The test and benchmark executables run
their framework checks separately. CTest coordinates native executable startup
and framework checks; cross-build validation must distinguish compilation from
execution on the target platform.

The cooker also provides `keygen` to create an Ed25519 private PEM/PKCS8 key and
raw public key for pack signing and independent runtime verification. Key files
use exclusive creation and owner-only permissions; see the
[key provisioning commands](content-pipeline.md#cooking-and-verification).

The cooker snapshots the source and directly referenced entity layers, composes
the private snapshot, validates the bounded contract and encodes local collider
recipes, derives the common build identity, and writes server, agent and client
packs in private staging. It reopens and verifies all completed files before
publishing their directory. The C++ content harness maps one file read-only,
verifies trusted-key authentication and scene encoding, then reports complete
scene values. Invalid input returns an error without partial content. See
[pack v1](../schemas/pack/v1.md) for the trust and publication boundaries.

After authentication, ResourceManager retains the immutable recipe and backing
storage. SceneWorld prepares complete transformed entities before synchronous
publication. Transfers preserve current placement and resource leases; unload
destroys only still-owned members. Explicit collection evicts manager-only
resources and advances generations. Resource handles are opaque identities:
callers compare or resolve them, while the manager owns issuance and storage
validation. See the [runtime lifetime contract](runtime-scenes.md) and
[local evidence](validation/runtime-scenes.md).

The [invalid-pack matrix](validation/invalid-packs.md) exercises this boundary
with tampering and independently signed malformed fixtures. It checks rejection
before the harness emits prepared values and checks pathname replacement while a
verified pack retains file ownership. This establishes the content boundary;
headless scene publication and unload are covered by #61, while application
world startup and SDK resource publication remain future behavior.

The pipeline reports cooking stages through an optional observer. The CLI owns
the terminal progress display on stderr and retains JSON results on stdout;
redirected stderr stays silent on success. Progress reaches completion only
after publication succeeds. See the
[cooker output contract](content-pipeline.md#cooking-and-verification).

The intended MVP flow is direct connection by IP address and port, entry at a
free predefined position, and independent play without waiting for another
player. Departure frees a slot while other players continue. Initial connection
attempts are limited to five seconds; five seconds without communication
triggers connection-loss handling. The specification defines focus handling,
server-full errors, and the other required runtime cases; none is implemented
yet.

The proposed client flow sequences commands from either input source, predicts
local movement against static geometry, and sends those same commands to the
server. Authoritative snapshots restore a prediction baseline and acknowledge
resolved inputs; remaining movement inputs are replayed. Dynamic state is
received, not simulated locally. The Presentation World consumes predicted local
poses, server-sampled remote poses, and deduplicated confirmed events.
Presentation smoothing never changes prediction or an agent's simulation
observation. The owner requires the Simulation and Prediction Worlds to run at
240 Hz with fixed 1/240 s steps, and the Presentation World to target 60 Hz
using variable elapsed time. The owner approved the per-world
[Flecs phase order](flecs-phases.md), with all phases describing world-level
data transformations at the same abstraction level. No I/O may execute inside
ECS systems or their transitive calls. Application orchestration performs
external operations and imports results between explicit ECS segments; thread
scheduling and concrete segmentation remain proposals. The
[phase contract proposal](phase-contracts.md) defines logical inputs/outputs and
permitted state changes for all 17 phases, including correlated external physics
requests/results. These contracts remain under review; protocol details, replay
limits, and correction tolerances remain open.

Before runtime world initialization, the external loader validates the cooked
pack signature, payload integrity, schemas, and compatibility and then supplies
immutable runtime content. Missing, unsigned, altered, or incompatible packs
prevent gameplay. The proposed admission handshake compares the common signed
content build identity shared by all consumer artifacts. Cooking and signing
occur offline; the [pack design](cooker-and-packs.md) records the unimplemented
details. The [runtime lifecycle proposal](runtime-lifecycle.md) defines
readiness gates, per-peer admission/removal, one client connection deadline
through usable-baseline preparation, focus behavior, and cleanup after partial
startup or in-flight work. These application states surround the existing ECS
phases and remain proposals.

## 7. Deployment view

The owner selected a [WSL Dev Container](development-container.md) for C++ and
Python. VS Code and native CI consume the same complete development image from
GHCR, pinned by digest in the editor configuration. A separate workflow
publishes candidates; reviewed PRs promote the digest after native validation.
CI verifies the image input fingerprint and never rebuilds on a cache miss. The
image's digest-pinned Ubuntu base, system-package lock and tool checksums define
its userspace. See the
[publication contract](development-container.md#published-image-and-updates).
System-package URLs pin an Ubuntu archive snapshot and retain mandatory hash
verification, so image construction does not depend on current mirror retention;
see the
[fixed-input contract](development-container.md#fixed-inputs-and-isolation). The
locked system tools include bubblewrap and GitHub CLI (`gh`) for repository
issue and pull-request operations. Generated build, Python, and JavaScript
directories use dedicated volumes; compiler and package caches persist
separately. CI prepares dependencies online and checks fresh builds with
networking disabled. Local
[container execution checks](validation/development-container.md) pass; kernel,
hardware, CodeQL extraction, and Windows SDK/runtime validation are separate
boundaries.

The editor forwards the developer's WSL SSH agent into the container; private
keys remain on the host. The
[SSH agent setup](development-container.md#forward-the-wsl-ssh-agent) documents
shell environment propagation, socket renewal, and fingerprint checks.

Local builds place outputs and dependency installations under build/. The
[GitHub workflow](../.github/workflows/ci.yml) runs Linux Debug,
ThreadSanitizer, and Release validation on Ubuntu 26.04 and retains diagnostic
artifacts. Windows cross-builds are local build targets; they are outside CI. No
application deployment or release publication pipeline exists yet.

The owner has requested LAN-only Kubernetes continuous deployment for the
simulation server: permanent develop and production environments, and temporary
feature, hotfix and release environments removed when their branches disappear.
All environments restart on deployment and may disconnect players. The
[CD design](continuous-deployment.md) records accepted behavior and remaining
implementation inputs. Single-node K3s runs directly on the Debian Dell R630
with MetalLB. The infrastructure provides a LAN IP and DNS name per environment
under `blackflower.home.arpa`, and a common UDP port. Flux reconciles deployment
state from a permanent `gitops` branch in this repository. The planned
application pipeline will use public digest-pinned GHCR server images based on
`ubuntu:26.04`. Server and content-pack publication will have independent
lifecycles, so multiple server versions may reuse the same pack. Server pods
will share a pack filesystem and select a pack by command-line argument. The
pack filesystem will reside on the Dell and be read-only for server pods;
publication must preserve packs in use. Windows client delivery is outside this
CD scope. Application deployment is unimplemented; diagnostic infrastructure
evidence is recorded in
[Dell operations](dell-operations.md#provisioning-evidence).

CD server artifacts must be compiled specifically for the actual Dell R630 CPU
and validated on that hardware. The
[Dell-specific build requirement](continuous-deployment.md#dell-specific-server-build)
records the specified dual Xeon E5-2690 v4 target and requires measured
performance. GitHub runners build Clang Release with `-O3`, `-march=broadwell`,
`-mtune=broadwell` and ThinLTO; numerical semantics remain intact. PGO is
deferred until representative profiles can be collected on the Dell and its
benefit measured.

Binary reference packs and their public key in tests/fixtures/packs are stored
with Git LFS. The build workflow downloads their contents during checkout; local
clones use the [Git LFS setup](git-workflow.md#starting-work). The versioned
pre-push hook uploads the referenced objects before publishing commits.

JavaScript actions in the build and CodeQL workflows use SHA-pinned releases
declaring the Node.js 24 runtime. The separate Node.js version installed for
source style checks follows the [style tooling policy](style-guidelines.md).

The agreed reference deployment places the authoritative server on the owner's
Dell R630 running Debian 13.6 and four concurrently rendering Windows client
processes on the owner's Lenovo P620 running Windows 11 Pro, connected by LAN.
Clients are cross-compiled on Linux and must be executed on Windows for
acceptance. The
[reference environment in the specification](https://github.com/sergioffpc/blackflower/issues/11)
records the owner-supplied CPU, RAM, and GPU details. The four clients share one
machine's resources; the setup does not establish performance on four
independent computers.

ServerScene, AgentScene and ClientScene are complete for their respective
consumers. The server selects `.bfserver`, autonomous participants select
`.bfagent`, and human clients select `.bfclient`. All three role scenes
currently contain only entities, including identical bounds. The autonomous
participant runtime and model-driven control remain outside the content
preparation scope. The loader receives a path and trusted keys without a role
selector. Each runtime receives an independently provisioned trusted public key
for verification. Private content-signing keys stay in the packaging
environment. No MVP deployment has been validated. Record actual OS builds,
drivers, build configuration, display/audio settings, and network conditions
with the first delivery evidence, and link operational instructions when
introduced.

The [C4 deployment view](c4.md#deployment-reference-acceptance-environment)
shows one server instance and four client instances. The proposed server uses
CPU PhysX and does not require the client rendering/audio stack. The client
requires GPU PhysX; its four prediction instances share the P620 GPU with
rendering. Linux-hosted Windows compilation must separate Linux tools from
Windows libraries and runtime artifacts; see the
[stack feasibility checks](technology-stack.md#evidence-required-before-the-first-delivery-can-rely-on-the-stack).

The [Dell infrastructure](dell-operations.md) introduces single-node K3s, Flux,
MetalLB and private LAN DNS. Source templates live under deploy/dell; Flux reads
operational state from the permanent gitops branch. A diagnostic UDP workload
exercises infrastructure independently of simulation-server readiness. Refer to
the operational evidence before claiming LAN acceptance.

## 8. Crosscutting concepts

### ECS execution and external effects

All ECS worlds process in-memory data only, as required by
[ADR-0004](adr/0004-keep-io-outside-ecs.md). Systems, observers, hooks, and
their helpers do not perform network, file/log, input-device, audio, rendering,
or GPU I/O. Adapters invoked by application orchestration outside ECS execution
own those operations, including PhysX calls and completion. Clocks, diagnostics,
external status, and SDK results cross the world interface as values. The
[proposed data contracts](phase-contracts.md) distinguish completed phase output
from requests awaiting external results, reject stale session/tick/request
identities, and keep replaceable frame state separate from retained feedback
events.

The [phase definitions](flecs-phases.md) use one abstraction level: world data
transformations with explicit inputs and outputs. SDK calls, device operations,
fences, codecs, and entity iteration are implementation/orchestration details.
Existing world rates and the GPU prediction requirement still apply to the
complete processing cycle, including external physics work.

### Errors and content ingestion

Project-owned error contracts use enums/classes under the
[typed-error policy](cpp-guidelines.md#typed-errors). Error identity is separate
from diagnostic text.

Source files, packs, manifests and provenance have no policy byte caps. The
loader validates actual byte ranges and representable sizes before slicing or
converting lengths, preserving signature and scene validation. File loading
retains a read-only mapping and allocates decoded scene values and the signed
metadata transcript separately. Hash verification reads all payload bytes.
Backing files must remain unchanged until all mapping owners release them; see
[ADR-0011](adr/0011-map-content-files.md).

### Development workflow

The [development process](development-process.md) defines Kanban, selected XP
practices, and fidelity validation for the team. Follow its operating limits and
completion criteria alongside the arc42 activities below.

Use the [build and dependency workflow](build.md),
[reference editor setup](editor.md), and
[Git-flow branch model](git-workflow.md) for implementation and integration.

GitHub enforces pull requests, signed commits, and required build and CodeQL
checks on main and develop, including administrators. The
[security policy](../SECURITY.md) defines confidential reporting for this public
repository. Secret scanning and push protection supplement the
[CodeQL and clang-tidy analysis](static-analysis.md). CodeQL analyzes production
targets only, excluding test and benchmark translation units as required by the
owner. It uses a separate Linux Release build with compiler caching disabled to
preserve extraction coverage; it does not replace sanitizer validation.

Apply the [arc42 method](https://arc42.org/method/) iteratively. The following
activities inform one another; use them at the level of detail warranted by the
change.

Use the [AI Hero agent workflow](agents/workflow.md) to move work through
clarification, specification, tickets, implementation, and review. The
activities below guide the architecture work within those stages. Tracker
operations and domain documentation follow the
[agent skills configuration](../AGENTS.md#agent-skills).

1.  Clarify the intended behavior, relevant constraints, and acceptance
    criteria. For architectural work, establish the important quality goals and
    measurable scenarios before choosing a solution.
2.  Design responsibilities and interfaces around the domain. Update the
    relevant context, building block, runtime, and deployment views.
3.  Identify concepts shared across modules, such as error handling,
    persistence, security, and observability, when they become relevant. Explain
    how they support the quality goals.
4.  Communicate significant choices and record their rationale in section 9.
    Keep open questions and proposals visible for feedback.
5.  Implement in small increments and review the code against the documented
    responsibilities and decisions. Update affected documentation in the same
    change when implementation produces a better design.
6.  Evaluate the result against acceptance criteria and quality scenarios using
    appropriate tests, measurements, or review. Record unresolved risks and use
    the findings to guide the next increment.

Run `npm --prefix tools/code_quality run check` alongside the language-specific
checks for changes to JavaScript, Markdown, JSON, or shell. The existing
required Linux Debug status includes this check; setup and scope are in the
[style policy](style-guidelines.md#installation-and-commands).

### Completion criteria

Apply the
[development process completion criteria](development-process.md#completion-criteria),
including acceptance checks, review evidence, current architecture
documentation, and recorded risks.

Repository language and commit rules are defined in [AGENTS.md](../AGENTS.md).

## 9. Architectural decisions

arc42 adoption is an explicit project convention recorded in
[AGENTS.md](../AGENTS.md). Selected product constraints and proposed integration
decisions are indexed below; the minimal content path is implemented while
runtime integration remains pending.

| Decision                                                                                                                                                                   | Status and source                                                                                                                                                                                                                |
| -------------------------------------------------------------------------------------------------------------------------------------------------------------------------- | -------------------------------------------------------------------------------------------------------------------------------------------------------------------------------------------------------------------------------- |
| Authoritative Linux server; Windows clients cross-compiled on Linux; LAN-only MVP                                                                                          | Required by the owner in [Define the MVP operating and connection conditions](https://github.com/sergioffpc/blackflower/issues/8), consolidated in the [MVP specification](https://github.com/sergioffpc/blackflower/issues/11). |
| Falcor, PhysX, Slang, Flecs, GameNetworkingSockets, and Steam Audio                                                                                                        | Selected by the owner; [stack integration and candidate pins](technology-stack.md) remain to be validated.                                                                                                                       |
| Two client worlds, common human/agent input, GPU prediction against static geometry, server-supplied dynamic state                                                         | [ADR-0002](adr/0002-separate-client-worlds-and-input.md), required by the owner; GPU movement integration and reconciliation details remain to be validated.                                                                     |
| Simulation and Prediction at 240 Hz fixed step; Presentation at 60 Hz target with variable step                                                                            | [ADR-0003](adr/0003-world-update-rates.md), required by the owner; scheduling and runtime performance remain unvalidated.                                                                                                        |
| No I/O inside ECS; phases express world-level data transformations                                                                                                         | [ADR-0004](adr/0004-keep-io-outside-ecs.md), required by the owner; approved phase order is documented in [world phases](flecs-phases.md).                                                                                       |
| Two C++ runtimes and one offline Python cooker using Assimp, meshoptimizer, and Slang-to-SPIR-V; three verified signed content packs per scenario required before gameplay | [ADR-0005](adr/0005-require-signed-cooked-content.md), required by the owner; primitive format and algorithms are implemented in ADR-0007; mesh/shader/SDK integration remains pending.                                          |
| OpenUSD scene input, uv-managed Python, initial primitive pack format, cryptography/libsodium and independent trust                                                        | [ADR-0007](adr/0007-minimal-pack-format-and-trust.md), accepted for #21.                                                                                                                                                         |
| Vulkan as the only graphics backend, with offline SPIR-V and no DirectX 12 build or fallback                                                                               | [ADR-0006](adr/0006-use-vulkan-only.md), required by the owner; Falcor integration remains unvalidated.                                                                                                                          |
| Contain Falcor exceptions inside the graphics adapter                                                                                                                      | [ADR-0001](adr/0001-isolate-falcor-exceptions.md), proposed; the current no-exceptions project policy remains in force.                                                                                                          |

The owner requires Google-derived style enforcement across C++, Python,
JavaScript, Markdown, JSON, and shell, including the complete staged commit
snapshot. The [style policy](style-guidelines.md) records the compatibility
exceptions and selected tooling; the required Linux Debug CI check includes the
same source checks and the commit-hook integration test.

[ADR-0008](adr/0008-pin-the-development-container.md) records the selected
development container, fixed inputs, and offline validation boundary.

[ADR-0009](adr/0009-typed-errors-and-content-size-policy.md) defines typed error
contracts and the content size policy.

[ADR-0010](adr/0010-agnostic-content-packs.md) defines the agnostic content-pack
contract and the pack v1 format.

[ADR-0011](adr/0011-map-content-files.md) selects read-only file mappings and
defines their ownership and immutable-backing-file contract.

[ADR-0012](adr/0012-use-flux-for-lan-cd.md) selects Flux, single-node K3s and
MetalLB for LAN server delivery, with public GHCR images and a same-repository
`gitops` state branch.

[ADR-0013](adr/0013-deploy-private-lan-services-with-flux.md) records the Dell
infrastructure choices, including private DNS integration and operational Git
separation. Application deployment automation remains unimplemented.

Record significant future decisions in docs/adr/ following the
[domain documentation rules](agents/domain.md), and index them here once
created. Capture the status, context, driving requirements, alternatives
considered, chosen approach, and consequences. When replacing a decision, retain
its rationale and link to the replacement.

The [USD entity authoring contract](usd-authoring.md) implements the first slice
of [#57](https://github.com/sergioffpc/blackflower/issues/57) through #58:
Scenes contain only entities with string placement identities and owned
`Bounds`. One relative definition reference per placement enables reuse.
Binary64 placement and oriented boxes preserve rotations without inferring
collision from visuals. #61 migrates those boxes from world-space Bounds to
local Collider recipes;
[ADR-0014](adr/0014-instantiate-local-scene-recipes-in-ecs.md) records shared
resources, ECS authority and lifetime trade-offs. Exact dependency snapshots
determine relocatable provenance. The
[ADR-0010 amendment](adr/0010-agnostic-content-packs.md#entity-contract-amendment)
supersedes the earlier collection and coordinate decisions. GLB visuals remain a
subsequent slice.

The owner selected a published GHCR development image with reviewed digest
promotion in [#42](https://github.com/sergioffpc/blackflower/issues/42). This
separates ordinary validation from upstream package availability; the
[publication contract](development-container.md#published-image-and-updates)
records access, updates, retention and rollback.

## 10. Quality requirements

Physical, acoustic, and visual realism are confirmed product priorities. Their
measurable acceptance thresholds, reference data, and trade-offs with
performance remain open. Training effectiveness also requires explicit learning
objectives and evaluation criteria. The
[validation process](development-process.md#fidelity-and-validation) describes
how to gather this evidence; no fidelity or training outcomes have been
validated yet.

For the first MVP, the owner agreed simple scene/player geometry and non-spatial
hit feedback, with these required acceptance scenarios. All thresholds are
targets awaiting evidence. The
[specification](https://github.com/sergioffpc/blackflower/issues/11) defines the
full reference configuration and verification procedure.

| ID      | Stimulus and operating conditions                                                                                        | Expected response and threshold                                                                                                                                         |
| ------- | ------------------------------------------------------------------------------------------------------------------------ | ----------------------------------------------------------------------------------------------------------------------------------------------------------------------- |
| MVP-Q01 | Four clients render at 1920 by 1080 each on the reference P620, connected to the Dell server during the five-minute run. | In each client separately, at least 95% of frame times are at most 16.7 ms.                                                                                             |
| MVP-Q02 | Players issue movement and valid hit actions over the reference LAN during that run.                                     | At least 95% of relevant movement observations and hit-sound responses occur within 100 ms of the originating action; assess behavior and receiving clients separately. |
| MVP-Q03 | One server and four rendering clients exercise the scenario continuously for five minutes.                               | No crash, hang, or unexpected disconnect; record MVP-Q01 and MVP-Q02 during the same run.                                                                               |
| MVP-Q04 | Initial connection fails, or established peer communication stops.                                                       | Limit initial connection attempts to five seconds; after five seconds without communication, detect loss and perform the specified error and cleanup behavior.          |

The following architecture checks are proposed validation cases for ADR-0002
through ADR-0005, not additions already published in the MVP issue. Exact
positional tolerances, history limits, and correction timing must be agreed
before implementation.

| ID       | Stimulus                                                                                                                                                                            | Observable check                                                                                                                                                                                                                                                                                                |
| -------- | ----------------------------------------------------------------------------------------------------------------------------------------------------------------------------------- | --------------------------------------------------------------------------------------------------------------------------------------------------------------------------------------------------------------------------------------------------------------------------------------------------------------- |
| ARCH-Q01 | Human and scripted-agent adapters produce the same command trace under the same snapshots and timing.                                                                               | Identical commands enter prediction and networking; switching the source adds no alternate simulation path.                                                                                                                                                                                                     |
| ARCH-Q02 | An authoritative correction resolves a pending movement prefix during player contact.                                                                                               | Resolved commands are removed; only remaining movement is replayed. No client-side dynamic collision response, duplicate shot, or duplicate hit sound occurs.                                                                                                                                                   |
| ARCH-Q03 | Change presentation interpolation while replaying the same input and authoritative snapshots.                                                                                       | Prediction results and agent simulation observations do not change.                                                                                                                                                                                                                                             |
| ARCH-Q04 | Exercise capsule movement against a wall on the P620 with four rendering clients.                                                                                                   | Runtime evidence shows the intended physics work executes on GPU and preserves static collision behavior; CPU fallback alone does not pass. Existing MVP-Q01–Q03 targets still apply.                                                                                                                           |
| ARCH-Q05 | Capture normal and replay ticks alongside presentation updates on the reference setup.                                                                                              | Simulation/prediction step values remain 1/240 s; presentation uses measured elapsed time with 60 Hz pacing. Record tick overruns, replay cost, actual cadences, and existing MVP frame/response results.                                                                                                       |
| ARCH-Q06 | Exercise all world phases with prepared input/results and inspect emitted data.                                                                                                     | No network, file/log, device, agent, GPU/render/audio, or PhysX SDK operation executes inside ECS callbacks or their helpers. Review dependencies and test real adapter orchestration separately.                                                                                                               |
| ARCH-Q07 | Supply duplicate, stale, or mismatched request results and retry exported output batches under the proposed phase contracts.                                                        | Incompatible results do not advance accepted state; repeated actions preserve identity; replacing frame data does not discard pending feedback. Check phase write permissions and output ownership as well as values.                                                                                           |
| ARCH-Q08 | Start from the signed cooked pack without source assets; repeat with tampering, malformed metadata, unknown signing key, incompatible content, and a mismatched peer content build. | Each target initializes from a valid signed pack with compatible resource schemas; peers compare the authenticated content build identity; invalid content is rejected before world initialization, and a peer content mismatch is rejected before admission. No asset cooking or pack I/O executes inside ECS. |
| ARCH-Q09 | Import an agreed static model with material/attribute seams, optimize it, encode it, and read the final packed data.                                                                | The proposed geometry-preserving optimization retains transformed surfaces, winding, required attributes, and material assignment. C++ runtime loading uses cooked data; repeat cooking with identical pinned inputs/settings and compare outputs.                                                              |
| ARCH-Q10 | Cancel/fail each startup stage, delay client admission/baseline, and stop with adapter work in flight.                                                                              | No gameplay before readiness; one proposed five-second connection deadline; acquired resources released once after users quiesce; no ECS I/O during startup/teardown. Measure shutdown duration; its limit remains open.                                                                                        |
| ARCH-Q11 | Inspect client build/artifact dependencies and run on the P620 using cooker-produced SPIR-V.                                                                                        | Only the Vulkan graphics backend is built and used; no DirectX 12 backend/runtime dependency or fallback and no source shader compilation. Unsupported required Vulkan capability fails startup.                                                                                                                |

Content validation follows the
[functional test scope](development-process.md#current-application-test-scope)
at the cooker-to-runtime boundary:

| ID          | Stimulus and operating conditions                                                                                                                                                           | Expected response and threshold                                                                                                                                                                                       |
| ----------- | ------------------------------------------------------------------------------------------------------------------------------------------------------------------------------------------- | --------------------------------------------------------------------------------------------------------------------------------------------------------------------------------------------------------------------- |
| CONTENT-Q01 | Cook an OpenUSD scene with dimensions and collection counts different from the reference; load each resulting file alone under an unrelated extension with independently provisioned trust. | All artifacts preserve entity identity, placement and owned bounds without scenario-specific quantities; the returned scene variant matches the authenticated magic and the build identity matches the cooker output. |

CONTENT-Q03: cook reused floor/box entity definitions, including a translated,
90-degree rotated and uniformly scaled box, then remove source files and load
all three signed packs. Preserve IDs after prim renaming and bounds ownership;
compare analytical centres/dimensions within 1e-9 metres and quaternion
components within 1e-12. Relocating the source tree preserves every pack byte;
changing definition bytes changes build identity. Optional entities and bounds
produce empty collections. See [local evidence](validation/scene-entities.md).

CONTENT-Q02, required by
[#22](https://github.com/sergioffpc/blackflower/issues/22): feed tampered,
truncated, untrusted and validly signed malformed artifacts to the actual C++
loader. Every invalid case must return a concrete error, emit no prepared
content and produce no sanitizer diagnostic. Replacing the source pathname must
preserve the already verified scene; Windows must deny replacement while the
mapping is held. Scope and evidence are in the
[invalid-pack validation](validation/invalid-packs.md).

For each agreed quality scenario, record a stable identifier, priority, stimulus
and source, affected part of the system, operating conditions, expected
response, and measurable acceptance threshold. Link it to the relevant design
and validation evidence. Group scenarios by quality goal when useful.

The STYLE-Q01 development quality scenario requires the full source style check
to return success with zero diagnostics for conforming files, and nonzero for
representative malformed JSON, invalid Markdown structure, and unsafe shell. Run
both the positive inventory and negative probes when changing its tools or
configuration. For C++, Python, JavaScript, Markdown, JSON, and shell, stage an
unformatted file while keeping a formatted working copy: the commit must fail
without changing either version. Correcting and staging the source must permit
the commit. This tests enforcement, not application behavior.

Validation of CONTENT-Q01 is recorded in the
[content evidence](validation/content-pipeline.md), including actual Windows
Debug ASan execution and a negative memory-error probe.

DEV-Q01 requires the isolated container to import the locked OpenUSD bindings
and pass native Debug, TSan, and Release checks from fresh build directories
after dependency preparation, with networking and compiler/vcpkg binary cache
reuse disabled. Debug also passes source style and hook tests. Record image
identity, revision, commands, and logs; a host build does not satisfy this
scenario. Local execution passed for #33; the
[validation record](validation/development-container.md) identifies the tested
image, source, commands, and remaining boundaries.

DEV-Q02: with no cached Docker image, native CI must pull the committed GHCR
digest and pass all three existing native presets without building an image or
downloading Ubuntu packages. A changed image input with an unchanged pin must
fail before dependency preparation. The editor configuration is the single
authority for the consumer reference. Validate publication and a fresh hosted
pull before accepting a new digest. See the
[GHCR validation boundary](validation/ghcr-development-image.md).

The infrastructure acceptance boundary is a signed Git change through DNS and
UDP on the LAN, including update and removal while preserving unrelated
resources. This diagnostic boundary does not validate simulation or content
readiness. [Dell operations](dell-operations.md) defines the reproducible steps
and recovery responsibilities.

The [CD acceptance targets](continuous-deployment.md#acceptance-targets), CD-Q01
through CD-Q04, cover deployment timing, removal, periodic cleanup and server
readiness. They remain unvalidated.

### Headless scene lifetime

Given the signed collision fixture and two instances of its recipe, root
placement reproduces analytical box geometry within 1e-9 metres and 1e-12
quaternion component tolerance. Updating one instance preserves the other;
transferred members survive their former instance's unload; final unload leaves
zero managed entities. Old entity and resource handles fail after reuse. Invalid
placement publishes no partial instance. These are local functional requirements
and evidence under [#61](validation/runtime-scenes.md), not physics, performance
or multi-world deployment results.

## 11. Risks and technical debt

The planned CD environments share one Dell host and local pack storage. Host
failure affects every environment, and server replacement failures require
manual recovery. The diagnostic lifecycle passed from the Lenovo through Google
Mesh DNS; see [LAN acceptance](dell-operations.md#lenovo-lan-acceptance). Shared
volume access, application automation and runtime readiness still need
implementation work; track them in the
[CD implementation inputs](continuous-deployment.md#implementation-and-provisioning-inputs)
before provisioning. Capacity and deployment timing require measurements on the
Dell.

| ID    | Open issue                                                                                                                                                                                                                                   | Impact                                                                                                                                          | Next step                                                                                                                                                                                                                                                                      |
| ----- | -------------------------------------------------------------------------------------------------------------------------------------------------------------------------------------------------------------------------------------------- | ----------------------------------------------------------------------------------------------------------------------------------------------- | ------------------------------------------------------------------------------------------------------------------------------------------------------------------------------------------------------------------------------------------------------------------------------ |
| R-001 | Training users, learning objectives, and operational use cases beyond the technical MVP are undefined.                                                                                                                                       | Completing the MVP will not establish training usefulness.                                                                                      | Establish these requirements with the owner before expanding the product.                                                                                                                                                                                                      |
| R-002 | Broader realism thresholds and reference data remain undefined; MVP quality targets are agreed but unmeasured.                                                                                                                               | Simple MVP acceptance cannot substantiate the broader realism goals.                                                                            | Measure the scoped MVP targets, then define reference evidence for any later fidelity claims.                                                                                                                                                                                  |
| R-003 | The desired scope may exceed a one- or two-person team's capacity.                                                                                                                                                                           | Work may expand faster than it can be integrated and validated.                                                                                 | Evaluate the small reference scene and observed delivery capacity before expanding the initial scope.                                                                                                                                                                          |
| R-004 | The Linux server and cross-compiled Windows clients have not been validated on the owner-supplied reference systems.                                                                                                                         | Build success alone may hide runtime or dependency compatibility failures.                                                                      | Execute both targets on the reference machines and capture exact environment details with acceptance evidence.                                                                                                                                                                 |
| R-005 | Four clients share one workstation, and end-to-end frame/sound timing evidence has not been collected.                                                                                                                                       | Resource contention or an incomplete timing method could hide failures of the agreed quality targets.                                           | Measure each rendering client and relevant output separately; preserve the focus policy and record timing methods and raw samples.                                                                                                                                             |
| R-006 | Falcor's inspected Windows build assumes Windows host tools, and its error interface conflicts with unrestricted inclusion in exception-free project code.                                                                                   | The selected rendering stack may require build adaptations and a deliberate error-handling policy before the first slice can work.              | Resolve [ADR-0001](adr/0001-isolate-falcor-exceptions.md), separate host/target dependencies, and prove the exact cross-build and Windows runtime path.                                                                                                                        |
| R-007 | Steam Audio does not select the application's device-output backend; miniaudio/WASAPI is proposed.                                                                                                                                           | The chosen SDK list does not yet fully specify audible playback and capture.                                                                    | Agree the output integration and verify per-process signal output on Windows before claiming audio acceptance.                                                                                                                                                                 |
| R-008 | GPU client prediction has not been built or measured; standard PhysX character-controller queries do not establish GPU movement.                                                                                                             | The selected movement technique may require adaptation; four physics instances compete with rendering and may disagree with CPU server results. | Prove static-only GPU movement, Windows GPU runtime compatibility, reconciliation, and sustained 240 Hz simulation/prediction alongside 60 Hz presentation; define correction tolerances and bounded replay before implementation.                                             |
| R-009 | Published tickets predate two client worlds and interchangeable input; agent observations and policy remain undefined.                                                                                                                       | Implementation could follow the older presentation-only design or expand into unspecified agent capabilities.                                   | Update affected ticket descriptions/validation against ADR-0002 before implementation; keep agent intelligence separate from the common input interface.                                                                                                                       |
| R-010 | Referenced entity/bounds cooking and signed loading are implemented; remaining integration includes Falcor Vulkan loading of offline Slang-compiled SPIR-V, resource-schema compatibility, and runtime preparation of GPU physics resources. | A runtime importer/compiler or incomplete signature coverage could violate the required deployment model.                                       | Extend the implemented bounded path with Assimp/meshoptimizer and native SDK artifacts, provision production content trust, and test gameplay admission against the common content build identity. Retain runtime validation and the current minimal functional test coverage. |
| R-011 | Runtime lifecycle orchestration and adapter cancellation are unimplemented; liveness mapping, input-age bounds, and finite shutdown deadlines remain open.                                                                                   | Startup races, stale movement, or outstanding SDK work could prevent safe admission or timely exit.                                             | Refine the [lifecycle proposal](runtime-lifecycle.md), update affected tickets, and validate partial startup and in-flight shutdown on both targets.                                                                                                                           |

R-012: Mapped ingestion avoids a full raw-byte heap copy but still touches the
whole payload for hashing and allocates decoded scene collections. Allocation
failure and mapped-page I/O faults are not recoverable typed errors. Linux
publishers must prevent writes or truncation while mappings exist; Windows
handles deny writes and deletion. Measure working sets and loading latency on
representative packs before choosing streaming or prefetch policies.

The Windows toolchain uses the dynamic release CRT for project code and
dependencies in both configurations. This resolves the LLVM 21.1.8 ASan startup
failure in the Microsoft Debug CRT while retaining Debug symbols, assertions,
unoptimized code and ASan. Microsoft debug heap and debug iterator checks are
not enabled; keep the runtime ABI consistent when adding dependencies. See the
[validation evidence](validation/content-pipeline.md).

The current checks validate build infrastructure and the bounded entity content
pipeline. They provide no evidence about simulation fidelity, performance
budgets, or training outcomes. Update this section as implementation risks are
discovered, mitigated, or resolved.

The development container passed DEV-Q01 locally on the recorded WSL kernel.
Package retention, Windows SDK provisioning, sanitizer behavior on other host
kernels, and host/container compilation timings remain separate validation
concerns in [#33](https://github.com/sergioffpc/blackflower/issues/33). Repeat
the offline checks when changing the image or its locked inputs; the local
results do not establish hosted CI execution.

GHCR availability, package permissions and digest retention now govern fresh
development-image pulls. Preserve referenced images and rollback revisions;
private packages require developer authentication and prevent unauthenticated
fork use. Rebuilding candidates still requires retained Ubuntu snapshots and
upstream tool archives. Independent image backups and a package mirror remain
unimplemented.

The Dell is a single point of failure for cluster workloads and LAN DNS through
the Google Mesh resolver. Keep private backups and the documented DNS fallback
procedure. A host-only UDP result does not establish reachability from a second
LAN machine.

The runtime scene foundation is synchronous and headless. Parent, visuals,
concurrent publication, streaming, world progression and cross-world identities
remain later #57 slices. Resource scans and simple recipe lists have no measured
large-scene budget. #61 builds on the local unpublished #58 commit; publishing
and integrating that dependency is still required before delivered-runtime or CI
claims. See [runtime scene evidence](validation/runtime-scenes.md) for the next
validation boundaries.

## 12. Glossary

The root [CONTEXT.md](../CONTEXT.md) is the authoritative glossary for input
sources, Simulation World, Prediction World, Presentation World, and
static/dynamic world terminology, maintained under the
[domain documentation rules](agents/domain.md).

## Template attribution

This document adapts the arc42 structure created by Gernot Starke and Peter
Hruschka, licensed under
[CC BY-SA 4.0](https://creativecommons.org/licenses/by-sa/4.0/). Template
guidance has been replaced with the project's initial status and working
conventions. See the [arc42 license information](https://arc42.org/license/).
