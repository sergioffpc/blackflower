# MVP cooker and signed content packs

Status: the owner requires two C++ runtimes (client and server), one offline
Python cooker using Assimp for 3D model import and meshoptimizer for mesh
optimization before final-format conversion, Slang for offline shader
compilation to SPIR-V, and two verified, digitally signed packs per scenario:
one for simulation and one for presentation. The minimal OpenUSD cooker, signed
primitive pack format and C++ loader are implemented by
[#21](https://github.com/sergioffpc/blackflower/issues/21); see the
[implemented pipeline](content-pipeline.md). Mesh, shader, GPU physics and audio
processing below remain proposals for subsequent tickets. See
[ADR-0005](adr/0005-require-signed-cooked-content.md), [C4](c4.md), and
[the technology stack](technology-stack.md).

The [cooker specification](https://github.com/sergioffpc/blackflower/issues/19)
records the required outcomes and owner-confirmed production-to-consumption test
boundary. Detailed implementation proposals below remain distinct from approved
constraints.

## Content flow

```mermaid
flowchart LR
    source["Source assets and scene definition"]
    cooker["Offline cooker on Linux"]
    signer["Signing step in the packaging tool"]
    presentationPack["Signed presentation pack"]
    simulationPack["Signed simulation pack"]
    source --> cooker --> signer
    signer --> presentationPack
    signer --> simulationPack
```

The owner selected Python for the offline Linux CLI tool, `blackflower-cooker`;
the client and server remain C++23. The cooker validates source assets and
references, produces portable runtime data, assembles a deterministic manifest,
signs the package, and verifies the finished artifact. Python and C++ loaders
conform to the same language-neutral format and shared test vectors; parser
source code is not assumed to be shared. The
[repository layout proposal](repository-layout.md) separates their source,
packages, and build outputs. Signing is an explicit packaging stage; runtime
applications only need verification capabilities and public keys.

The cooker produces two independently signed packs: simulation content for the
Simulation and Prediction Worlds, and presentation content for the Presentation
World. Their formats are independent of the deployment platform. The server
loads simulation content; the client loads both roles. Each artifact can be
verified without reading its counterpart.

Simulation content contains world rules, collision geometry and interaction
data. Presentation content contains meshes, materials, textures, SPIR-V shaders
and audio. Portable resource schemas define their representations; runtime
adapters prepare platform-specific resources. Resource identities and shared
scene values stay consistent across the two content roles. Prediction remains
subject to its existing limits on client-side simulation.

The implemented primitive slice contains only the shared validated scenario
resource in each independently signed artifact. The source scene is OpenUSD, and
uv manages the Python environment and locked dependencies. Different extensions
do not replace authenticated role checks.

## Selected cooker stack and model processing

| Responsibility             | Owner-selected technology | Role                                                                                                             |
| -------------------------- | ------------------------- | ---------------------------------------------------------------------------------------------------------------- |
| Language and orchestration | Python with uv            | Offline CLI, processing stages, validation, packaging, and signing orchestration.                                |
| Scene source               | OpenUSD                   | Read the self-contained USDA/USDC primitive scene through official Python bindings.                              |
| 3D model import            | Assimp                    | Read supported source models and expose their geometry, scene transforms, and material references to the cooker. |
| Mesh optimization          | meshoptimizer             | Optimize imported mesh data before conversion to the runtime format.                                             |
| Shader compilation         | Slang                     | Compile all required shader entry points and variants to SPIR-V offline for the presentation pack.               |

The canonical library name is `meshoptimizer`. Assimp exposes C/C++ interfaces
and Python bindings; its upstream PyAssimp wrapper uses `ctypes` and requires
the native shared library. The wrapper documents incomplete coverage, so its
availability is not proof of compatibility with the selected model features.
meshoptimizer exposes a C-compatible interface suitable for native interop.
Sources: [Assimp](https://github.com/assimp/assimp),
[PyAssimp](https://github.com/assimp/assimp/tree/master/port/PyAssimp),
[meshoptimizer](https://github.com/zeux/meshoptimizer).

The libraries are selected; Python bindings/FFI, exact native versions,
interpreter compatibility, supported input formats, and packaging remain to be
validated together on the Linux build host. These are offline cooker
dependencies. They do not introduce another product runtime or require the game
to import source models.

Proposed model path:

```text
Source model
  -> Assimp import
  -> validated canonical model data
  -> meshoptimizer optimization
  -> final runtime mesh encoding
  -> pack assembly, signature, and verification
```

The canonical intermediate model is cooker-owned memory, independent of Assimp
handles and the eventual runtime file layout. Normalize coordinate/scale
conventions and primitive representation once, preserve node transforms,
material partitions, and all required vertex attributes, and validate indices
and references before optimization. Missing materials or unsupported content
features are explicit cooking errors or documented conversion choices; they must
not disappear silently.

For the initial MVP, propose exact full-vertex deduplication, vertex-cache
ordering, then vertex-fetch ordering within compatible mesh/material partitions.
Remap every affected vertex attribute together and preserve triangle winding and
surface geometry. Overdraw ordering can be added where measured; simplification,
lossy quantization, LOD generation, meshlets, and encoded mesh compression are
separate choices rather than automatic consequences of selecting this library.
The upstream
[optimization pipeline](https://github.com/zeux/meshoptimizer#core-pipeline)
informs the ordering.

This starting policy preserves the agreed relationship between visible geometry,
static collision geometry, and hit queries. PhysX preparation consumes the same
validated scene geometry; render-buffer reordering must not change collision
dimensions or move surfaces. The pack stores the final prepared buffers; this
proposal does not require a meshoptimizer runtime decoder.

Record source hashes, Assimp/meshoptimizer and binding revisions,
import/optimization settings, and output-format version in cooking provenance.
Prove Python-to-native buffer ownership, repeated-build reproducibility,
attribute/material preservation, and consumption of the final pack by the C++
loader. Assimp and meshoptimizer integration is outside the implemented
primitive slice.

## Offline shader compilation

The Python cooker invokes a pinned Linux-host Slang compiler to produce SPIR-V
modules, with entry-point, stage, binding/reflection, specialization, and
target-capability metadata needed by the runtime. Source shaders and includes
are cooking inputs. Enumerate and cook all required MVP shader variants; missing
variants fail preparation rather than triggering runtime compilation. Record
compiler revision, arguments, include dependencies, and target profile in
provenance. Slang documents the `spirv` output target in its
[getting-started guide](https://docs.shader-slang.org/en/stable/external/slang/docs/user-guide/01-get-started.html).

SPIR-V is the selected delivered shader format. Vulkan is the only permitted
graphics backend, as required by [ADR-0006](adr/0006-use-vulkan-only.md);
[Falcor 8.0](https://github.com/NVIDIAGameWorks/Falcor/blob/8.0/README.md)
advertises Vulkan support, but loading these precompiled modules and their
metadata without invoking its runtime compiler remains unproven. Validate the
pinned Slang version, SPIR-V/Vulkan profile, resource layouts, and
cross-compiled client on the P620. Creating device shader modules and pipelines
remains an external runtime operation; source compilation belongs to the cooker.

## What is cooked

| Content   | Pack data                                                                                                                                     | Runtime operation allowed after validation                                             |
| --------- | --------------------------------------------------------------------------------------------------------------------------------------------- | -------------------------------------------------------------------------------------- |
| Scenario  | Geometric primitives, lights, spawn points, stable resource identities, and validated references                                              | Instantiate project-owned scene values and ECS entities.                               |
| Rendering | Prepared mesh buffers, material parameters, and any required texture data in a selected runtime format                                        | Create/upload resources through the external Falcor adapter.                           |
| Physics   | Validated primitive definitions; pre-cooked mesh data only where the chosen physics representation needs it, including GPU data when required | Create PhysX scenes/shapes from prepared representations; no mesh cooking from source. |
| Shading   | Offline Slang-compiled SPIR-V modules and required binding/reflection metadata for the client Vulkan/Slang profile                            | Create shader/pipeline resources using those artifacts.                                |
| Audio     | The short hit signal as prepared PCM with explicit sample format, rate, and channel count                                                     | Create playback buffers in the external audio adapter.                                 |

Analytic primitives are encoded directly without a mesh-cooking stage.
Participant dimensions belong to participant configuration. The source and
cooked schemas have explicit versions and conversions. No scene importer,
source-shader compiler, texture converter, or PhysX mesh cooker runs in the
delivered MVP application path. Driver processing required to create GPU
resources is distinct from application asset cooking; normal resource
initialization remains necessary.

Falcor's previously inspected path invokes Slang at runtime, so consumption of
precompiled shader artifacts is an additional feasibility requirement, not an
established feature of this integration. Prove the appropriate load path or
adapt it before claiming cooked-only startup. Linux-host preparation of Windows
shader/PhysX GPU artifacts must use the selected toolchain and target formats.
PhysX describes cooked mesh formats in its
[geometry documentation](https://nvidia-omniverse.github.io/PhysX/physx/5.5.0/docs/Geometry.html);
exact target/version compatibility still needs testing.

## Implemented primitive pack format

[Pack v1](../schemas/pack/v1.md) is the normative byte contract: explicit
little-endian fields, a canonical manifest, one primitive scenario payload,
SHA-256 digests and a detached Ed25519 signature over the exact header/manifest
transcript. [Scene v1](../schemas/scene/v1.md) defines OpenUSD authoring and the
runtime primitive layout. The C++ loader owns the bytes it verifies and returns
a complete value only after structural, cryptographic and scene validation.
Failures are typed values, with diagnostic text rendered separately. There are
no policy byte caps on source files, packs, manifests or provenance;
[ADR-0009](adr/0009-typed-errors-and-content-size-policy.md) records the
decision and reader compatibility.

Python uses cryptography 46.0.5; C++ uses libsodium 1.0.22#1 through vcpkg and a
scoped Windows cross-build overlay.
[ADR-0007](adr/0007-minimal-pack-format-and-trust.md) records these selections,
replacing the earlier OpenSSL EVP proposal for the C++ loader. Independent
literal-encoded reference fixtures check the two implementations. Extensions are
`.bfpresentation` and `.bfsimulation`.

## Scenario compatibility

`PackId` identifies one artifact and therefore differs between simulation and
presentation. Both signed headers contain a common `ScenarioBuildId`. The cooker
derives it from a canonical build description containing the scenario inputs,
common scene/rule contract, both content roles, pinned tool/settings revisions,
and both roles' cooked resource digests. Exclude signatures, final `PackId`
values, and the build identifier itself to avoid circular hashing. The exact
encoding is specified in pack v1.

Admission compares `ScenarioBuildId`, not equality of the two `PackId` values.
Each loader checks the required content role and resource schemas. The cooker
verifies pair agreement before publication. Rejecting peers with a different
build is a subsequent admission responsibility, not a local loader comparison
against a second pack. This conservative MVP policy also rejects a
presentation-only recook paired with an older simulation pack. More permissive
compatibility is a future decision. Scene revision alone does not identify the
complete build.

## Signing trust and production contracts

A dedicated content-signing private key belongs to the packaging environment,
outside the repository and distributed artifacts. It is separate from Git commit
signing. The runtime has an independently provisioned trusted public-key set; a
key embedded in a pack cannot authorize itself. The loader accepts an externally
supplied trust set; the harness reads a caller-selected raw public-key file.
Production executable provisioning and rotation remain future deployment
choices. Automated tests use disposable signing keys; only public keys and
signed reference artifacts are committed.

| Logical contract | Input                                                                                      | Output and owner                                                                                         |
| ---------------- | ------------------------------------------------------------------------------------------ | -------------------------------------------------------------------------------------------------------- |
| Cook content     | `CookRequest`: source set, scene definition, both content roles, pinned tool settings      | Two role-specific `CookedContentSet` values and a common build description, owned by the offline cooker. |
| Package/sign     | Both cooked sets, canonical manifests with common `ScenarioBuildId`, signing configuration | Two independently signed artifacts plus a pair verification report, owned by packaging.                  |

Publish the pair only after both artifacts pass verification and agree on their
scenario build identity and common scene/rule values. A failed cook or signing
step must not publish an incomplete pair as a successful build. The
[runtime lifecycle](runtime-lifecycle.md#content-preparation-boundary) owns
verification/loading, SDK preparation, and world initialization; these are not
cooker responsibilities.

## MVP validation and delivery impact

The first playable slice needs the signed pack pair, role-aware loading and
resource initialization. Functional checks cover each role's standalone
verification and the runtime's required content. Follow the
[current test scope](development-process.md#current-application-test-scope).

Run the delivered applications without source assets available and establish
that no application asset cooking occurs. Exercise actual GPU physics and
precompiled shader loading on the P620. Record startup cost separately from the
accepted five-minute performance run. Signing establishes content origin under
the chosen trust key; cooking repeatability and runtime compatibility require
their own evidence.

Outstanding work includes production public-key provisioning, portable resource
schemas, Assimp/meshoptimizer bindings and exact versions, native SDK tool
integration, and the Falcor precompiled-shader path. The required outcome is two
verified, independently signed packs per scenario, with offline Slang-compiled
SPIR-V in the presentation pack.
