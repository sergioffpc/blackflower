# MVP technology stack

Status: the owner selected the six main technologies below. The owner also requires separate Prediction and Presentation Worlds with interchangeable human/agent input, GPU client physics against static geometry only, and server-supplied dynamic state. Detailed placement and supporting integration choices are proposed; exact cross-build compatibility has not been demonstrated. This document complements the [MVP specification](https://github.com/sergioffpc/blackflower/issues/11), [arc42 architecture](architecture.md), and [C4 views](c4.md).

## Selected technologies and proposed placement

| Responsibility | Owner-selected technology | Proposed placement and MVP use |
| --- | --- | --- |
| Rendering | NVIDIA Falcor | Windows client: window/render integration, simple scene geometry, player representation, camera, and centered marker. Start with its D3D12 path as an integration proposal. |
| Shaders | Slang | Use the version paired with the selected Falcor revision. Client rendering stays simple; selecting the framework does not add advanced rendering requirements. |
| Physics | NVIDIA PhysX | CPU collision, character movement, and scene queries on the Linux authoritative server; required GPU prediction of local movement against static geometry in each Windows client. Dynamic simulation and collision outcomes remain on the server. Separate physics instances share rules and geometry. Windows GPU PhysX cross-compilation and actual GPU execution require proof. |
| ECS | Flecs | One authoritative Simulation World on the server; two separate worlds in each client: Prediction World and Presentation World. Human and autonomous input use the same command pipeline. Network player identifiers remain independent of Flecs's local entity identifiers. |
| Networking | Valve GameNetworkingSockets | Both executables: direct-IP LAN connections, transport delivery and connection state. This is the interpretation of the owner's “gameservicesocket”. |
| Audio processing | Steam Audio | Windows audio module. The MVP hit signal remains non-spatial and restricted to the two involved players. Spatial effects stay bypassed. |

The owner requires the Simulation and Prediction Worlds to run at 240 Hz with a fixed 1/240 s step, and the Presentation World to target 60 Hz with variable elapsed time. [World phases](flecs-phases.md) describe the proposed scheduling; network transmission rates remain separate.

The implementation baseline remains C++23, Clang 21, CMake, Ninja, sccache, vcpkg, GoogleTest, Google Benchmark, and the repository's analysis/sanitizer workflow. Prefer standard-library facilities and Boost for remaining gaps; the owner's explicit library choices above take precedence over a default library preference.

All three ECS worlds are restricted to in-memory processing. Application-owned adapters invoke the selected SDKs outside ECS execution, as required by [ADR-0004](adr/0004-keep-io-outside-ecs.md).

The MVP also requires an [offline cooker and signed content pack](cooker-and-packs.md). The cooker runs on Linux; runtime loaders verify signatures, integrity, and compatibility before creating worlds. SHA-256 and Ed25519 through OpenSSL EVP are integration proposals, not yet selected package pins. Shader and physics target artifacts must be prepared offline and loaded without runtime asset cooking.

## Integration proposals

**Audio output:** add miniaudio's device layer with WASAPI for the Windows client. Steam Audio's documented integration expects a mixer/output engine; miniaudio supplies that device connection. The audio module owns their integration and the short PCM signal. The additional library is proposed, not yet agreed. Sources: [Steam Audio integration](https://valvesoftware.github.io/steam-audio/doc/capi/integration.html), [miniaudio low-level device interface](https://miniaud.io/docs/manual/index.html).

**Falcor error handling:** isolate Falcor-facing code behind one narrow module interface that returns explicit errors to the application. Falcor exposes exception-handling code; allowing exceptions in that adapter would be a deliberate, local change to the project policy. The [proposed ADR](adr/0001-isolate-falcor-exceptions.md) records the precise scope. The current exception restriction remains in force until the owner resolves this proposal.

**Build integration:** keep vcpkg as the application dependency entry point. Use pinned overlay ports or a pinned external-package integration where the selected framework requires it. Falcor's Packman dependency graph and scripts need explicit host/target separation before they can fit the Linux-hosted Windows build. The actual integration mechanism and patches remain to be proven, rather than assumed from upstream Windows support.

**GPU prediction:** the client requirement supersedes the earlier CPU/server-only placement. The inspected PhysX 5.5 [GPU documentation](https://nvidia-omniverse.github.io/PhysX/physx/5.5.0/docs/GPURigidBodies.html) puts scene queries on CPU. A standard character controller does not become a GPU movement implementation by enabling scene flags. Validate a suitable GPU rigid-body movement path, the required Windows GPU binaries, and the P620 driver before fixing the PhysX pin. CPU fallback does not satisfy the requested GPU proof.

**Linux deployment:** build/link the server against a runtime baseline compatible with the reference Debian installation. The current Ubuntu development host does not itself establish Debian binary compatibility. Rendering and audio dependencies belong to the client target and must not become server deployment requirements.

## Candidate versions to validate

| Technology | Candidate from inspected sources | Pinning rule |
| --- | --- | --- |
| Falcor | 8.0, the upstream revision inspected for this proposal | Pin its source revision, dependency manifests, binary package hashes, and necessary build adaptations together. |
| Slang | 2024.1.34 in Falcor 8.0's dependency manifest | Start with the framework's paired version. Upgrade through compatibility testing rather than selecting an unrelated latest release. |
| PhysX | 5.5.0, port revision 1, in the existing vcpkg baseline | Validate CPU-only Linux server packaging and GPU-enabled Windows cross-build/runtime support, including the reference GPU and driver, before adoption. |
| Flecs | 4.1.6 in the existing baseline | Verify the chosen interface with the project's exact flags. |
| GameNetworkingSockets | 1.6.0 in the existing baseline | Disable optional ICE/P2P for the direct-IP LAN MVP; preserve its required crypto/serialization dependencies. |
| Steam Audio | 4.8.1 in the existing baseline | Validate the Windows library and its runtime/interface boundary. |
| miniaudio | 0.11.25 in the existing baseline | Supporting dependency pending the owner's choice. |

These are candidate pins, not installed packages or a claim that all combinations work. Sources and detailed limitations are in the [rendering/physics research](research/nvidia-rendering-physics-compatibility.md) and [Flecs/Valve research](research/valve-flecs-compatibility.md). The [Falcor dependency manifest](https://github.com/NVIDIAGameWorks/Falcor/blob/8.0/dependencies.xml) couples Falcor 8.0 to Slang 2024.1.34; the [existing registry baseline](https://github.com/microsoft/vcpkg/blob/9e593bb18ea69cc5095e012465dcd675a822ed0d/versions/baseline.json) supplies the other listed port candidates.

## Evidence required before the first delivery can rely on the stack

1. Compile and link a minimal Falcor-facing Windows executable from Linux using the actual Clang/xwin target, host-native tools, and target Windows libraries. Prove the selected exception boundary with a deliberate dependency-error case.
2. Execute it on the P620 and render a Slang-backed primitive with the pinned Falcor dependency set. Record the driver, SDK, runtime files, and graphics backend actually used.
3. Build the CPU-only PhysX/Flecs server slice and execute it on the Dell under the declared Debian runtime. Cross-compile the GPU PhysX prediction slice and run it on Windows. Prove actual GPU movement/collision against static geometry with the agreed capsule and slide behavior; receiving dynamic state must not trigger local dynamic simulation. Compare CPU-server/GPU-client corrections rather than assuming identical results.
4. Exchange direct-IP messages between the Linux and Windows binaries using GameNetworkingSockets. Verify required transitive dependencies and explicit error results.
5. Emit and isolate a non-spatial signal in the Windows audio module with the agreed device backend. Do not equate an audio command being queued with sound output.

These checks belong at the start of [Enter the fixed scene from a Windows client](https://github.com/sergioffpc/blackflower/issues/12), with physics and audio evidence added as their delivery slices begin. If a cross-build adaptation becomes too large for that ticket, split a bounded feasibility investigation before beginning gameplay implementation. The original behavioral acceptance criteria remain authoritative. [ADR-0002](adr/0002-separate-client-worlds-and-input.md) adds the client-world/input separation: verify identical command traces from human and scripted-agent adapters, authoritative reconciliation, presentation isolation, and exactly-once confirmed feedback. Update affected delivery tickets before implementation; agent policy and observation contents remain open.
