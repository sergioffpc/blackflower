# Instantiate local scene entity descriptions directly in ECS

Status: accepted and amended for #61 under the owner-approved #57 specification.
The original multi-instance portion is superseded by the
[single-scene World amendment](#single-scene-world-amendment). Local
implementation and evidence do not imply that the prerequisite #58 branch has
been published or integrated.

USD is the offline authoring representation; verified SceneAssets contain
immutable scene entity descriptions; Flecs owns live mutable entity state.
SceneInstance stores source ownership and membership only. ResourceManager
deduplicates compiled collider definitions by AssetId, and entity-owned leases
preserve resources through transfer and unload. This avoids a second mutable
scene graph and heavy geometry copies per live instance.

The existing authoring name `Bounds` continues to mean collision and maps to
Collider, while LocalBounds and WorldBounds are reserved for visual culling.
Schema 1 changes in place from world-space boxes to local boxes plus placement.
This supersedes the world-space part of the
[ADR-0010 entity amendment](0010-agnostic-content-packs.md#entity-contract-amendment).
Pre-baking world boxes would prevent shared definitions and risk applying
placement twice; local collider descriptions preserve orientation while
supporting independent root placement. Development fixtures are regenerated,
without a legacy decoder.

The [schema](../../schemas/scene/v1.md) defines domain-separated content hashes,
SceneEntityId namespaces and coordinates. The
[runtime contract](../runtime-scenes.md) defines typed generations, leases,
transfer, failure and exhaustion. Synchronous exclusive publication is
sufficient for this headless slice; asynchronous staging and multi-world
activation remain later work. Retaining explicit ownership and validation costs
some memory and preparation work, with no scale claim until measured.

The owner explicitly selected GLM 1.0.3 for quaternion/vector arithmetic in this
slice, replacing Boost.QVM and overriding the general Boost-first fallback.
Portable content uses the binary32 XYZW convention selected by the later
[precision amendment](0010-agnostic-content-packs.md#precision-amendment); the
implementation converts to GLM's constructor order at the arithmetic boundary.

## Single-scene World amendment

On 2026-09-11, the owner established the domain invariant that one live World
contains exactly one Scene. Empty state exists only while the World is being
loaded or unloaded. This supersedes this decision's `SceneInstance` namespace,
optional instance-root placement, member transfer, and independently unloadable
multiple instances inside one World. Those mechanisms expressed unsupported
generality rather than a product requirement.

`SceneWorld` is now the live namespace. It loads one SceneAsset atomically,
returns an optional Entity from GetEntity by SceneEntityId, and unloads the
whole Scene. A second load is rejected without changing the active Scene.
Authored placement is already in World coordinates for root entities, so no
separate Scene root transform is retained. Immutable collider resources may
still be deduplicated across the separate Simulation, Prediction, and
Presentation Worlds; sharing a definition does not share mutable ECS state.

This smaller interface makes the one-to-one ownership rule explicit and removes
membership state and operations that could violate it. It also means that moving
an entity between Scenes requires an explicit future cross-World protocol, not
an in-World membership mutation.
