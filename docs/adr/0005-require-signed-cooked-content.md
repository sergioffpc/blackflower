# Require verified signed packs of cooked content

Status: accepted for the signed cooked-content requirement, the offline Python
cooker, offline Slang-to-SPIR-V compilation, and two role-specific packs per
scenario, explicitly requested by the owner. Pack format, cryptographic
algorithms, and tool integration below remain proposals.

The MVP comprises two C++23 runtimes, client and server, and one offline Python
cooker using the owner-selected Assimp importer and meshoptimizer optimizer
before final-format conversion. The cooker also uses Slang to compile shaders to
SPIR-V offline. It produces two independently signed packs per scenario: client
content for Prediction/Presentation and server content for Simulation. Each
runtime consumes only its own pack. Server and clients validate the pack before
creating their worlds or admitting gameplay; they do not cook source assets
during startup or simulation. Pack I/O, signature/integrity checks, decoding,
and SDK resource creation execute outside ECS, preserving
[ADR-0004](0004-keep-io-outside-ecs.md).

This makes the cooker and pack loader part of the first delivery instead of
deferring the content pipeline. It introduces artifact compatibility and
signing-key management, while providing an explicit content identity and
rejecting altered or unauthenticated inputs. Loading loose source assets or
silently bypassing failed verification would violate this requirement. The
[cooker proposal](../cooker-and-packs.md) defines the intended artifact, trust
model, and remaining feasibility work.

The selected library roles are fixed; Python bindings, version pins, packaging,
and SDK-tool integration remain to be selected. Python and C++ implementations
share the pack specification and conformance vectors, rather than assuming one
parser implementation. The [directory proposal](../repository-layout.md)
preserves this separation.

The owner-selected pack split replaces the earlier proposal for one shared
artifact. Artifact identities therefore differ; the proposed common scenario
build identity and admission check are described in the cooker design. SPIR-V
delivery replaces unspecified shader artifacts; Vulkan is required by
[ADR-0006](0006-use-vulkan-only.md); consumption of cooked SPIR-V through Falcor
still requires integration proof. The
[runtime lifecycle proposal](../runtime-lifecycle.md) owns loading, resource
preparation, world startup, and shutdown.
