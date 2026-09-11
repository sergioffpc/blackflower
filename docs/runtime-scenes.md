# Runtime collision scenes

Status: implemented locally for
[#61](https://github.com/sergioffpc/blackflower/issues/61), the headless Phase 1
of [#57](https://github.com/sergioffpc/blackflower/issues/57). The
[validation record](validation/runtime-scenes.md) distinguishes local evidence
from integration and target-platform execution.

## Ownership and public boundary

The external application verifies a pack's signature and integrity with
independently provisioned trust, then calls ResourceManager.Load with the
VerifiedPack. The manager publishes an immutable SceneAsset and returns a typed
SceneHandle. Resolve returns a shared owning lease; handles alone do not pin
resources. The source pack, including its mapped backing bytes, remains owned by
the SceneAsset. Identical compiled scenes may reuse the first published pack's
backing storage; callers retain their own VerifiedPack when they need each
load's distinct role or provenance record. Deduplication never replaces
verification.

SceneWorld owns a real minimal Flecs world and is the namespace for exactly one
active Scene. Load takes a SceneHandle, validates all transformed geometry,
acquires shared collider leases and reserves project generations before creating
any live entity. Only a complete Scene becomes externally observable on return;
a second Load returns `kSceneAlreadyLoaded` without changing it. Empty state
exists only before Load and after Unload. There are no user systems/observers
registered in this headless API, and every operation requires exclusive access
outside world progression. SceneWorld does not expose a mutable Flecs world that
could bypass its ownership invariants. This #61 foundation accepts
collision-bearing descriptions only and returns `kMissingCollider` for a
presentation-only description. The role-specific runtime work under #76 must
project ClientScene before constructing its separate worlds; passing the
unprojected Client union to this collision world is not a supported shortcut.

The World retains source ownership and one live SceneEntityId-to-Entity map. It
contains no mutable pose or geometry tree outside ECS. Flecs components own all
live state; ReadEntityState returns a disposable copy for observation.
SetTransform updates the authoritative LocalTransform and its derived
WorldTransform together, after validating world colliders. No Parent hierarchy
or separate Scene root is implemented; root entities interpret their authored
LocalTransform in world space. Derived transforms are updated eagerly at these
explicit mutation points, not recomputed per frame.

| Component or description    | Domain and authority                                                               |
| --------------------------- | ---------------------------------------------------------------------------------- |
| SceneEntityId and placement | Shared immutable description; each world loads its own mutable storage.            |
| LocalTransform              | Shared spatial component definition; the owning ECS entity is authoritative.       |
| WorldTransform              | Derived spatial component; never written back to LocalTransform.                   |
| Collider                    | Simulation/Prediction collision reference; immutable geometry lives in resources.  |
| Identity                    | Runtime lifetime metadata, independent of gameplay and presentation.               |
| ColliderLease               | In-memory ECS ownership pin; no SDK or mapped-file cleanup in component callbacks. |

Collision resources are portable oriented boxes, not PhysX objects. Application
adapters will prepare SDK objects outside ECS. This foundation does not
implement world progression, simulation, prediction reconciliation or
presentation. The three-world topology, fixed 240 Hz Simulation/Prediction and
60 Hz Presentation target remain the established requirements.

## Lifetime and failure policy

GetEntity takes a SceneEntityId because SceneWorld supplies the single Scene
namespace. It returns `nullopt` when the World is empty or no live member has
that identity. Destroy removes a member from both ECS and the Scene map. Unload
destroys all remaining live members, clears the map and releases the Scene's
source lease.

Repeated Unload returns kNoScene. Repeated Destroy or stale entity access
returns kStaleEntity. CollectUnused, called by the external owner at a
synchronization point, evicts manager-only resources. External leases and
remaining ECS members protect their resources. Releasing a lease does not
automatically evict a resource. Keep the manager alive until all its worlds have
been destroyed; its handles are scoped to that manager lifetime and must never
be reused with a reconstructed manager at the same address.

Resource handles are opaque, copyable identities issued only by the manager;
default construction produces an invalid handle. Callers can compare handles and
resolve them without depending on their storage representation. Internally, each
handle contains an originating manager, slot and u64 generation. Zero generation
is invalid. Eviction advances generation; exhausting u64 retires that slot
instead of wrapping. Project Entity handles pair Flecs' entity ID with a
process-unique u64 generation stored in ECS. Zero is invalid. The allocator
refuses exhaustion with kIdentityExhausted, so even Flecs ID recycling cannot
alias a previous project handle. Foreign-world entities fail validation. These
are runtime identities, never persistent asset IDs or network identities.

Invalid entity transforms, overflowed derived geometry and identity collisions
are typed recoverable failures. Preparation failure releases temporary leases
without collecting unrelated resources; existing handles and any active Scene
remain intact. Prepared immutable resources may remain in the manager cache
until the owner explicitly calls CollectUnused, with no failed-load leases
retained. No asynchronous work, cancellation or concurrent resource publication
is supported. Allocation failure, mapped-page faults and Flecs aborts remain
process failures, not a promised transactional recovery path. The scoped #57
failure/handle tests do not expand the repository's general test policy.

## Dependencies and limits

The pinned vcpkg baseline supplies Flecs 4.1.6 and GLM 1.0.3. Flecs was already
selected by the owner; its minimum world supplies actual ECS storage. The
standard library has no quaternion composition/rotation facility. The owner
explicitly selected [GLM](https://github.com/g-truc/glm) instead of Boost.QVM
for this work, overriding the default Boost preference. The runtime uses GLM's
header-only target with no transitive runtime dependencies; vcpkg's CMake
helpers are build-only dependencies. Binary32 vectors/quaternions match the
schema's precision, with explicit XYZW-to-WXYZ constructor conversion. Used
arithmetic is allocation-free and has no shared mutable state or throwing path.
Clang/C++23 and no-exception compatibility are checked by the linked validation
commands.

Flecs
[entity generations](https://www.flecs.dev/flecs/md_docs_2EntitiesComponents.html)
are supplemented by project generations to define exhaustion and world
isolation. The manager currently scans its resource slots and the scene uses a
simple description list. These choices serve the collision fixture, with no
100,000-entity timing or memory claim. Chunk streaming, Parent, scripts,
visuals, async publication and cross-world binding belong to later tickets.
