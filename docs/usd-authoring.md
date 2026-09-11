# USD entity authoring

Status: entity authoring, local collider descriptions, and sparse role-scene
projection are implemented through #78. Headless ECS lifetime comes from
[#61](https://github.com/sergioffpc/blackflower/issues/61). Visual import and
textured content remain in #23 and #59 under the
[entity specification](https://github.com/sergioffpc/blackflower/issues/57).

## Implemented contract

A Scene describes spatial content and contains only an optional `Entities`
scope. A Scenario defines an exercise and its objectives separately. Scene files
use an identity-transform `/Scene` default Xform with
`custom int blackflower:schema = 1`, `metersPerUnit = 1` and `upAxis = "Y"`.

Each placement under `/Scene/Entities` is an Xform with exactly one relative
local external USD reference to a reusable definition. Definitions live under
`entities/`, use an identity-transform `/Entity` default Xform and the same
explicit units and up axis. References may select `/Entity` explicitly or use
the layer default prim. Definitions cannot reference further layers.

Each placement authors a unique, nonempty, case-sensitive ASCII string
`blackflower:id` matching `[A-Za-z0-9][A-Za-z0-9_.:-]*`. This becomes its
`SceneEntityId`. Definitions cannot supply IDs. Renaming a placement preserves
its ID; copying one requires another ID. The cooker sorts entities by ID.

Definitions may contain an optional `Bounds` scope with independently authored
Cube prims using `PhysicsCollisionAPI`. The collision-enabled value must be
true. Cubes have finite positive size and dimensions. An entity may have no
bounds or several; the floor uses a thin box. Bounds belong directly to their
entity in the internal content scene. The authored name `Bounds` maps to
Collider, never to visual culling LocalBounds or WorldBounds.

A nonempty `Bounds` scope authors exactly one
`custom token blackflower:collisionDomain`, either `SessionStatic` for fixed
scenario geometry or `AuthoritativeDynamic` for collision that belongs only to
the authoritative Simulation scene. Dynamic behavior and lifecycle are not
encoded by this contract.

Definitions may author optional logical presentation references directly on
`/Entity` as `custom string blackflower:visual` and
`custom string blackflower:audio`. Values use the same ASCII grammar as
`SceneEntityId`. These values identify future adapter-owned resources; they are
not source asset paths and do not add media bytes to the pack. Visual meshes
never determine collision. An optional empty `Visuals` scope remains accepted;
nonempty visuals and asset-typed attributes are rejected until visual import is
implemented. There are no Lights or Spawns scopes.

The cooker projects every supported source scene into three sparse catalogues.
ServerScene retains both collision domains and omits presentation references.
AgentScene retains `SessionStatic` collision only and omits presentation
references. ClientScene retains the union of `SessionStatic` collision, visual,
and audio entities; `AuthoritativeDynamic` collider data is physically omitted.
Descriptions without a domain for a role are omitted; retained descriptions keep
their authored `SceneEntityId`.

Entity placements support translation, rotateXYZ in degrees and positive uniform
scale, in that order; any of these operations may be omitted. Bound children
support the same order with positive nonuniform scale. Reset stacks, pivots,
inverse operations, matrices and other transform operations are unsupported. The
cooker preserves oriented boxes by storing entity placement separately from
entity-local collider transforms. Both use binary64 metre coordinates and unit
XYZW quaternions. Runtime instantiation composes the optional instance root,
placement and local box exactly once; see [Scene v1](../schemas/scene/v1.md).

The bounded subset rejects sublayers, payloads, variants, inherits, specializes,
instancing, relationships, animation, attribute connections, unsupported physics
APIs and unsupported prim types. Nested definition references and absolute
reference paths are unsupported. The cooker snapshots each input's exact bytes
before composition; provenance covers sorted logical dependency names and
content, independently of the absolute checkout location. Authors must not edit
sources during cooking: capture is per file, not an atomic filesystem snapshot.

Headless runtime instances support independent placement changes, member
transfer, destruction and full unload through the
[runtime scene API](runtime-scenes.md). No interaction parameters, ballistics or
runtime physics SDK execution are implemented here.

## Verification

The installed CLI cooks the floor and rotated box fixtures into three signed
packs, which the actual C++ loader consumes after all source files are removed.
A five-entity role fixture covers collision-only, visual-only, audio-only,
mixed, and authoritative-dynamic descriptions. Checks cover sparse role domains,
dynamic collision filtering, shared identity, reused definitions, stable IDs
after a prim rename, optional bounds and entities, and byte-identical outputs
after relocation. Editing a dependency changes the build identity. Analytical
box comparisons allow 1e-9 metres for position/dimensions and 1e-12 for
quaternion components; these are fixture tolerances, not global fidelity
guarantees.

## Scene example

Paths assume sibling `entities/` and `scenes/` directories.

The file `scenes/mvp.usda` places a floor and a box:

```usda
#usda 1.0
(
    defaultPrim = "Scene"
    metersPerUnit = 1
    upAxis = "Y"
)

def Xform "Scene"
{
    custom int blackflower:schema = 1

    def Scope "Entities"
    {
        def Xform "Floor" (
            prepend references = @../entities/floor.usda@
        )
        {
            custom string blackflower:id = "floor-main"
        }

        def Xform "Box" (
            prepend references = @../entities/box.usda@
        )
        {
            custom string blackflower:id = "box-01"
            double3 xformOp:translate = (0, 0.5, 0)
            double3 xformOp:rotateXYZ = (0, 30, 0)
            double3 xformOp:scale = (1, 1, 1)
            uniform token[] xformOpOrder = [
                "xformOp:translate",
                "xformOp:rotateXYZ",
                "xformOp:scale"
            ]
        }
    }
}
```

The file `entities/box.usda` defines a unit box collider:

```usda
#usda 1.0
(
    defaultPrim = "Entity"
    metersPerUnit = 1
    upAxis = "Y"
)

def Xform "Entity"
{
    def Scope "Bounds"
    {
        custom token blackflower:collisionDomain = "SessionStatic"

        def Cube "Body" (
            prepend apiSchemas = ["PhysicsCollisionAPI"]
        )
        {
            double size = 1
            bool physics:collisionEnabled = true
        }
    }
}
```

The floor definition follows the same structure. Its unit Cube uses local
translation `(0, -0.1, 0)` followed by scale `(20, 0.2, 20)`, placing the top
surface at Y = 0. Runnable fixtures are in
[scenes/entities.usda](../tests/integration/fixtures/scenes/entities.usda).

## Pending visual contract

Visual representation will serve Presentation; colliders serve Prediction and
Simulation. The implemented logical `blackflower:visual` reference proves the
role boundary but does not define model data. Reusable definitions will later
declare GLB sources under `Visuals` using `blackflower:sourceAsset`; this
declaration requests cooker import, not native USD composition. Supported
meshes, materials and texture dependencies remain to be selected in #23 and #59.
The visual model must align independently with the entity's local frame. General
domain terms live in [CONTEXT.md](../CONTEXT.md).
