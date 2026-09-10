# USD entity authoring

Status: the initial scope and authoring structure below are agreed. Remaining
contract decisions are open; this design is not implemented by the current
cooker. The implemented subset remains documented in
[Scene v1](../schemas/scene/v1.md).

The [entity specification](https://github.com/sergioffpc/blackflower/issues/57)
tracks delivery through entity collision (#58), GLB meshes/materials (#23) and
the textured scene (#59).

## Agreed initial scope

Author a scene containing a floor and a textured box, each with its own
collision geometry. Entities remain stationary in this iteration. Future
interaction, ballistics, movement and destruction motivate preserving entity
identity, but their behavior and parameters are outside this iteration.

Start with existing GLB models and hand-authored USDA. Reusable entity
definitions reference visual models; scene instances have their own identities
and placement transforms. Visual representation serves Presentation; collision
geometry serves Prediction and Simulation. Collision is explicitly authored and
optional in the general contract; both initial example entities include it.

Visual meshes never determine collision geometry. Collision geometry must be
authored independently; neither automatic extraction nor fitting from visual
meshes is part of the contract.

Initial colliders are boxes with explicit local dimensions and placement. An
entity definition can contain multiple colliders belonging to the same entity.
The floor uses a thin box. Instances support translation, rotation and positive
uniform scale, with metres and Y up. The instance transform applies to visual
and collision representations together, preserving their local alignment.

GLB materials and textures define appearance in this iteration, without USDA
material overrides. The supported material subset remains to be selected;
unsupported properties must produce a clear diagnostic.

Validate the USD-to-cooker-to-signed-packs-to-consumer path, preserving entity
identities, referenced visual content and placement transforms, and checking the
corresponding collision geometry. Rendering in a client is outside this
iteration's acceptance boundary. Detailed assertions and tolerances remain to be
specified.

## Agreed authoring structure

A Scene describes spatial content. A Scenario defines a training exercise,
including objectives, and has a separate contract outside this iteration. Each
scene file uses `/Scene` as its identity-transform default prim; `scenes/`
contains scene files. This replaces `/Scenario` in the proposed authoring path,
while the current cooker still requires its implemented `/Scenario` root.

Reusable definitions live under `entities/`, with `/Entity` as their default
prim. A scene places definitions through USD references beneath
`/Scene/Entities`. Each placement authors a nonempty string `blackflower:id`,
unique in the scene and independent of its prim name or path. Renaming or moving
a placement retains its ID; duplicating it requires another ID. Definitions do
not supply placement IDs. Runtime identity encoding remains open.

Definitions contain optional `Visuals` and `Colliders` scopes. A visual Xform
uses the custom asset attribute `blackflower:sourceAsset` to request GLB import
by the cooker; USD itself does not import that asset as scene composition.
Colliders are explicit Cube prims with `PhysicsCollisionAPI`, without an
associated rigid body in this iteration. Local collider scale defines box
dimensions independently of the uniform scale of the containing entity instance.

Placement transforms use translation, `rotateXYZ` in degrees, and positive
uniform scale, in that authored transform order. Visual and collider children
may have their own local transforms. No `instanceable` metadata is required.

## Scene example

Paths assume sibling `entities/`, `models/` and `scenes/` directories. The model
paths are illustrative inputs, not assets supplied by this document.

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
            prepend references = @../entities/textured-box.usda@
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

The file `entities/textured-box.usda` defines a unit box collider independently
of its visual model:

```usda
#usda 1.0
(
    defaultPrim = "Entity"
    metersPerUnit = 1
    upAxis = "Y"
)

def Xform "Entity"
{
    def Scope "Visuals"
    {
        def Xform "Model"
        {
            custom asset blackflower:sourceAsset = @../models/box-textured/BoxTextured.glb@
        }
    }

    def Scope "Colliders"
    {
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

The floor definition follows the same structure, referencing
`../models/floor/Floor.glb`. Its unit Cube collider uses local translation
`(0, -0.1, 0)` followed by scale `(20, 0.2, 20)`, placing the top surface at Y
= 0. The visual model must be aligned independently with that local frame.

## Open contract decisions

-   Supported visual model contents, materials and texture dependencies.
-   ID character rules and cooked/runtime identity encoding.
-   Allowed USD composition subset, dependency resolution and validation rules.
-   Cooked representations and their distribution across consumer packs.

Resolve these decisions before implementing the contract. General domain terms
live in [CONTEXT.md](../CONTEXT.md).
