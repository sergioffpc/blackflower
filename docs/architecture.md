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
harness. Simulation behavior has not been implemented.

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
two independently verified signed packs per scenario from the MVP onward: client
for Prediction/Presentation and server for Simulation, with Slang compiling
client shaders to SPIR-V offline; see the
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

| Building block                                                       | Responsibility                                                                                                                      |
| -------------------------------------------------------------------- | ----------------------------------------------------------------------------------------------------------------------------------- |
| [Console bootstrap](../src/main.cc)                                  | Print the project name and return a startup status.                                                                                 |
| [GoogleTest harness](../tests/build_test.cc)                         | Exercise test integration and the C++23 build contract.                                                                             |
| [Google Benchmark harness](../benchmarks/framework_benchmark.cc)     | Exercise benchmark registration and execution.                                                                                      |
| [Offline cooker](../tools/cooker/src/blackflower_cooker/pipeline.py) | Validate self-contained OpenUSD, encode primitive content, sign and verify both artifacts, then publish their directory atomically. |
| [Content module](../src/modules/content/content.h)                   | Own bounded pack bytes and authenticate their role/profile, manifest and payload before returning validated scene values.           |
| [Content harness](../tests/content_harness.cc)                       | Consume either pack alone using independent public-key trust; expose IDs and dimensions for cross-language integration checks.      |
| [Build configuration](../CMakeLists.txt)                             | Build four executables and the content library; run analysis, Python checks and integration tests.                                  |

The frameworks are linked only into their respective harnesses.
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

The cooker reads the source once, validates and encodes primitive geometry,
derives the common build identity, and writes two separately signed artifacts in
private staging. It reopens and verifies both completed files before publishing
the pair directory. The C++ content harness reads one bounded file into owned
storage, verifies trusted-key authentication and scene invariants, then reports
complete scene values. Invalid input returns an error without partial content.
See [pack v1](../schemas/pack/v1.md) for the trust and publication boundaries.

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
scenario build identity, since client and server pack identities differ. Cooking
and signing occur offline; the [pack design](cooker-and-packs.md) records the
unimplemented details. The [runtime lifecycle proposal](runtime-lifecycle.md)
defines readiness gates, per-peer admission/removal, one client connection
deadline through usable-baseline preparation, focus behavior, and cleanup after
partial startup or in-flight work. These application states surround the
existing ECS phases and remain proposals.

## 7. Deployment view

Local builds place outputs and dependency installations under build/. The
[GitHub workflow](../.github/workflows/ci.yml) runs Linux Debug,
ThreadSanitizer, and Release validation on Ubuntu 26.04 and retains diagnostic
artifacts. Windows cross-builds are local build targets; they are outside CI. No
application deployment or release publication pipeline exists yet.

The agreed reference deployment places the authoritative server on the owner's
Dell R630 running Debian 13.6 and four concurrently rendering Windows client
processes on the owner's Lenovo P620 running Windows 11 Pro, connected by LAN.
Clients are cross-compiled on Linux and must be executed on Windows for
acceptance. The
[reference environment in the specification](https://github.com/sergioffpc/blackflower/issues/11)
records the owner-supplied CPU, RAM, and GPU details. The four clients share one
machine's resources; the setup does not establish performance on four
independent computers.

The MVP requires a role-specific signed cooked-content pack alongside each
runtime binary, and an independently provisioned trusted public key for
verification. Private content-signing keys stay in the packaging environment. No
MVP deployment has been validated. Record actual OS builds, drivers, build
configuration, display/audio settings, and network conditions with the first
delivery evidence, and link operational instructions when introduced.

The [C4 deployment view](c4.md#deployment-reference-acceptance-environment)
shows one server instance and four client instances. The proposed server uses
CPU PhysX and does not require the client rendering/audio stack. The client
requires GPU PhysX; its four prediction instances share the P620 GPU with
rendering. Linux-hosted Windows compilation must separate Linux tools from
Windows libraries and runtime artifacts; see the
[stack feasibility checks](technology-stack.md#evidence-required-before-the-first-delivery-can-rely-on-the-stack).

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

Run `npm --prefix tools/style run check` alongside the language-specific checks
for changes to JavaScript, Markdown, JSON, or shell. The existing required Linux
Debug status includes this check; setup and scope are in the
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

| Decision                                                                                                                                                         | Status and source                                                                                                                                                                                                                |
| ---------------------------------------------------------------------------------------------------------------------------------------------------------------- | -------------------------------------------------------------------------------------------------------------------------------------------------------------------------------------------------------------------------------- |
| Authoritative Linux server; Windows clients cross-compiled on Linux; LAN-only MVP                                                                                | Required by the owner in [Define the MVP operating and connection conditions](https://github.com/sergioffpc/blackflower/issues/8), consolidated in the [MVP specification](https://github.com/sergioffpc/blackflower/issues/11). |
| Falcor, PhysX, Slang, Flecs, GameNetworkingSockets, and Steam Audio                                                                                              | Selected by the owner; [stack integration and candidate pins](technology-stack.md) remain to be validated.                                                                                                                       |
| Two client worlds, common human/agent input, GPU prediction against static geometry, server-supplied dynamic state                                               | [ADR-0002](adr/0002-separate-client-worlds-and-input.md), required by the owner; GPU movement integration and reconciliation details remain to be validated.                                                                     |
| Simulation and Prediction at 240 Hz fixed step; Presentation at 60 Hz target with variable step                                                                  | [ADR-0003](adr/0003-world-update-rates.md), required by the owner; scheduling and runtime performance remain unvalidated.                                                                                                        |
| No I/O inside ECS; phases express world-level data transformations                                                                                               | [ADR-0004](adr/0004-keep-io-outside-ecs.md), required by the owner; approved phase order is documented in [world phases](flecs-phases.md).                                                                                       |
| Two C++ runtimes and one offline Python cooker using Assimp, meshoptimizer, and Slang-to-SPIR-V; two verified signed packs per scenario required before gameplay | [ADR-0005](adr/0005-require-signed-cooked-content.md), required by the owner; primitive format and algorithms are implemented in ADR-0007; mesh/shader/SDK integration remains pending.                                          |
| OpenUSD scene input, uv-managed Python, bounded primitive pack v1, cryptography/libsodium and independent trust                                                  | [ADR-0007](adr/0007-minimal-pack-format-and-trust.md), accepted for #21.                                                                                                                                                         |
| Vulkan as the only graphics backend, with offline SPIR-V and no DirectX 12 build or fallback                                                                     | [ADR-0006](adr/0006-use-vulkan-only.md), required by the owner; Falcor integration remains unvalidated.                                                                                                                          |
| Contain Falcor exceptions inside the graphics adapter                                                                                                            | [ADR-0001](adr/0001-isolate-falcor-exceptions.md), proposed; the current no-exceptions project policy remains in force.                                                                                                          |

The owner requires Google-derived style enforcement across C++, Python,
JavaScript, Markdown, JSON, and shell, including the complete staged commit
snapshot. The [style policy](style-guidelines.md) records the compatibility
exceptions and selected tooling; the required Linux Debug CI check includes the
same source checks and the commit-hook integration test.

Record significant future decisions in docs/adr/ following the
[domain documentation rules](agents/domain.md), and index them here once
created. Capture the status, context, driving requirements, alternatives
considered, chosen approach, and consequences. When replacing a decision, retain
its rationale and link to the replacement.

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

| ID       | Stimulus                                                                                                                                                                                                   | Observable check                                                                                                                                                                                                                                                                                                                       |
| -------- | ---------------------------------------------------------------------------------------------------------------------------------------------------------------------------------------------------------- | -------------------------------------------------------------------------------------------------------------------------------------------------------------------------------------------------------------------------------------------------------------------------------------------------------------------------------------- |
| ARCH-Q01 | Human and scripted-agent adapters produce the same command trace under the same snapshots and timing.                                                                                                      | Identical commands enter prediction and networking; switching the source adds no alternate simulation path.                                                                                                                                                                                                                            |
| ARCH-Q02 | An authoritative correction resolves a pending movement prefix during player contact.                                                                                                                      | Resolved commands are removed; only remaining movement is replayed. No client-side dynamic collision response, duplicate shot, or duplicate hit sound occurs.                                                                                                                                                                          |
| ARCH-Q03 | Change presentation interpolation while replaying the same input and authoritative snapshots.                                                                                                              | Prediction results and agent simulation observations do not change.                                                                                                                                                                                                                                                                    |
| ARCH-Q04 | Exercise capsule movement against a wall on the P620 with four rendering clients.                                                                                                                          | Runtime evidence shows the intended physics work executes on GPU and preserves static collision behavior; CPU fallback alone does not pass. Existing MVP-Q01–Q03 targets still apply.                                                                                                                                                  |
| ARCH-Q05 | Capture normal and replay ticks alongside presentation updates on the reference setup.                                                                                                                     | Simulation/prediction step values remain 1/240 s; presentation uses measured elapsed time with 60 Hz pacing. Record tick overruns, replay cost, actual cadences, and existing MVP frame/response results.                                                                                                                              |
| ARCH-Q06 | Exercise all world phases with prepared input/results and inspect emitted data.                                                                                                                            | No network, file/log, device, agent, GPU/render/audio, or PhysX SDK operation executes inside ECS callbacks or their helpers. Review dependencies and test real adapter orchestration separately.                                                                                                                                      |
| ARCH-Q07 | Supply duplicate, stale, or mismatched request results and retry exported output batches under the proposed phase contracts.                                                                               | Incompatible results do not advance accepted state; repeated actions preserve identity; replacing frame data does not discard pending feedback. Check phase write permissions and output ownership as well as values.                                                                                                                  |
| ARCH-Q08 | Start from the signed cooked pack without source assets; repeat with tampering, malformed metadata, unknown signing key, incompatible content, and a mismatched peer scenario build and a wrong-role pack. | Each target initializes using only its own valid role pack; matching client/server packs have different artifact identities but the same scenario build identity; invalid content is rejected before world initialization, and a peer content mismatch is rejected before admission. No asset cooking or pack I/O executes inside ECS. |
| ARCH-Q09 | Import an agreed static model with material/attribute seams, optimize it, encode it, and read the final packed data.                                                                                       | The proposed geometry-preserving optimization retains transformed surfaces, winding, required attributes, and material assignment. C++ runtime loading uses cooked data; repeat cooking with identical pinned inputs/settings and compare outputs.                                                                                     |
| ARCH-Q10 | Cancel/fail each startup stage, delay client admission/baseline, and stop with adapter work in flight.                                                                                                     | No gameplay before readiness; one proposed five-second connection deadline; acquired resources released once after users quiesce; no ECS I/O during startup/teardown. Measure shutdown duration; its limit remains open.                                                                                                               |
| ARCH-Q11 | Inspect client build/artifact dependencies and run on the P620 using cooker-produced SPIR-V.                                                                                                               | Only the Vulkan graphics backend is built and used; no DirectX 12 backend/runtime dependency or fallback and no source shader compilation. Unsupported required Vulkan capability fails startup.                                                                                                                                       |

The following bounded content scenario is implemented by #21 and checked at the
agreed cooker-to-runtime seam:

| ID          | Stimulus and operating conditions                                                                                                                                                                                                            | Expected response and threshold                                                                                                                                                                                                                                                 |
| ----------- | -------------------------------------------------------------------------------------------------------------------------------------------------------------------------------------------------------------------------------------------- | ------------------------------------------------------------------------------------------------------------------------------------------------------------------------------------------------------------------------------------------------------------------------------- |
| CONTENT-Q01 | Cook the reference OpenUSD scene on Linux; load each role alone with independently provisioned trust on Linux and Windows. Inject signed malformed content, tampering and failure of either output/signature/verification/publication stage. | Both valid artifacts expose exactly the 20,000 mm interior, 1,800 by 600 mm capsule, two blocks and four canonical spawn values; PackIds differ and ScenarioBuildIds match. Every invalid-content case yields no scene, and failed pair creation yields no published directory. |

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

## 11. Risks and technical debt

| ID    | Open issue                                                                                                                                                                                                                        | Impact                                                                                                                                          | Next step                                                                                                                                                                                                                                                   |
| ----- | --------------------------------------------------------------------------------------------------------------------------------------------------------------------------------------------------------------------------------- | ----------------------------------------------------------------------------------------------------------------------------------------------- | ----------------------------------------------------------------------------------------------------------------------------------------------------------------------------------------------------------------------------------------------------------- |
| R-001 | Training users, learning objectives, and operational use cases beyond the technical MVP are undefined.                                                                                                                            | Completing the MVP will not establish training usefulness.                                                                                      | Establish these requirements with the owner before expanding the product.                                                                                                                                                                                   |
| R-002 | Broader realism thresholds and reference data remain undefined; MVP quality targets are agreed but unmeasured.                                                                                                                    | Simple MVP acceptance cannot substantiate the broader realism goals.                                                                            | Measure the scoped MVP targets, then define reference evidence for any later fidelity claims.                                                                                                                                                               |
| R-003 | The desired scope may exceed a one- or two-person team's capacity.                                                                                                                                                                | Work may expand faster than it can be integrated and validated.                                                                                 | Evaluate the small reference scene and observed delivery capacity before expanding the initial scope.                                                                                                                                                       |
| R-004 | The Linux server and cross-compiled Windows clients have not been validated on the owner-supplied reference systems.                                                                                                              | Build success alone may hide runtime or dependency compatibility failures.                                                                      | Execute both targets on the reference machines and capture exact environment details with acceptance evidence.                                                                                                                                              |
| R-005 | Four clients share one workstation, and end-to-end frame/sound timing evidence has not been collected.                                                                                                                            | Resource contention or an incomplete timing method could hide failures of the agreed quality targets.                                           | Measure each rendering client and relevant output separately; preserve the focus policy and record timing methods and raw samples.                                                                                                                          |
| R-006 | Falcor's inspected Windows build assumes Windows host tools, and its error interface conflicts with unrestricted inclusion in exception-free project code.                                                                        | The selected rendering stack may require build adaptations and a deliberate error-handling policy before the first slice can work.              | Resolve [ADR-0001](adr/0001-isolate-falcor-exceptions.md), separate host/target dependencies, and prove the exact cross-build and Windows runtime path.                                                                                                     |
| R-007 | Steam Audio does not select the application's device-output backend; miniaudio/WASAPI is proposed.                                                                                                                                | The chosen SDK list does not yet fully specify audible playback and capture.                                                                    | Agree the output integration and verify per-process signal output on Windows before claiming audio acceptance.                                                                                                                                              |
| R-008 | GPU client prediction has not been built or measured; standard PhysX character-controller queries do not establish GPU movement.                                                                                                  | The selected movement technique may require adaptation; four physics instances compete with rendering and may disagree with CPU server results. | Prove static-only GPU movement, Windows GPU runtime compatibility, reconciliation, and sustained 240 Hz simulation/prediction alongside 60 Hz presentation; define correction tolerances and bounded replay before implementation.                          |
| R-009 | Published tickets predate two client worlds and interchangeable input; agent observations and policy remain undefined.                                                                                                            | Implementation could follow the older presentation-only design or expand into unspecified agent capabilities.                                   | Update affected ticket descriptions/validation against ADR-0002 before implementation; keep agent intelligence separate from the common input interface.                                                                                                    |
| R-010 | Primitive OpenUSD cooking and signed loading are implemented; remaining integration includes Falcor Vulkan loading of offline Slang-compiled SPIR-V, role-specific pair compatibility, and target-specific GPU physics artifacts. | A runtime importer/compiler or incomplete signature coverage could violate the required deployment model.                                       | Extend the implemented bounded path with Assimp/meshoptimizer and native SDK artifacts, provision production content trust, and test gameplay admission against the common scenario build identity. Keep the primitive conformance and invalid-pack checks. |
| R-011 | Runtime lifecycle orchestration and adapter cancellation are unimplemented; liveness mapping, input-age bounds, and finite shutdown deadlines remain open.                                                                        | Startup races, stale movement, or outstanding SDK work could prevent safe admission or timely exit.                                             | Refine the [lifecycle proposal](runtime-lifecycle.md), update affected tickets, and validate partial startup and in-flight shutdown on both targets.                                                                                                        |

The Windows toolchain uses the dynamic release CRT for project code and
dependencies in both configurations. This resolves the LLVM 21.1.8 ASan startup
failure in the Microsoft Debug CRT while retaining Debug symbols, assertions,
unoptimized code and ASan. Microsoft debug heap and debug iterator checks are
not enabled; keep the runtime ABI consistent when adding dependencies. See the
[validation evidence](validation/content-pipeline.md).

The current checks validate build infrastructure and the bounded primitive
content pipeline. They provide no evidence about simulation fidelity,
performance budgets, or training outcomes. Update this section as implementation
risks are discovered, mitigated, or resolved.

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
