# Blackflower

Blackflower represents participants interacting in a shared training scenario.
This glossary defines the terms used to distinguish participant control,
provisional state, and perceived results.

## Language

**Input source**: The origin of a participant's movement, look, and firing
intentions, supplied by either a human or an autonomous agent. Both express the
same kinds of actions under the same simulation rules.

**Simulation World**: The official shared scenario state against which
participant actions and their outcomes are accepted. It is the authoritative
world maintained by the server. _Avoid_: Game World as a name for this world.

**Prediction World**: A client's provisional view of the scenario, advanced
using its participant's inputs and collisions against the static scenario while
awaiting authoritative results. Corrections from the Simulation World take
precedence over its predictions.

**Presentation World**: A client's view of the scenario prepared for perception,
including the participant's viewpoint and the visible and audible results.
Presentation adjustments do not change simulation outcomes.

**Static world**: The scenario geometry that remains fixed during a session,
such as the ground, enclosure, and fixed blocks.

**Dynamic world**: The changing scenario state, including participants and
movable objects, whose official evolution and interactions are determined by the
Simulation World. A client's local movement prediction remains provisional.

**Collider**: An entity's independently authored collision geometry. Colliders
define surfaces or volumes for interaction with the world and never derive from
the entity's visual representation. _Avoid_: Bounds as a runtime collision term;
`Bounds` remains the authoring scope name.

**CollisionDomain**: The participation class attached to a cooked collider
description. `SessionStatic` identifies fixed scenario collision that may enter
Simulation and Prediction. Authored dynamic collision remains undefined.

**VisualReference**: A logical identity for an entity's presentation visual. It
is neither media data nor a graphics SDK handle.

**AudioReference**: A logical identity for an entity's authored presentation
audio. It is neither encoded audio nor an audio SDK handle.

**Scene entity**: An individually identifiable entity placed in a scene, with an
optional visual representation and colliders. Its placement does not imply that
it can never move or be destroyed.

**Entity definition**: A reusable description of a scene entity's visual
representation and colliders, shared by its independently placed instances.

**Scene**: The spatial description of an environment, including placed entities,
with their optional visual representation and colliders. It is independent of
exercise objectives and gameplay rules.

**Scenario**: A training exercise definition that includes objectives. It is
distinct from the spatial description provided by a Scene; its detailed contract
remains to be defined.

**Cooked content**: Scene resources prepared before deployment in the
representation required by the runtime, rather than source assets awaiting
conversion.

**Content pack**: The signed distribution artifact containing a coherent set of
cooked scene resources and their compatibility and integrity metadata.

**ContentBuildId**: The shared identity of a produced set of cooked content,
covering its source, production settings and resources across consumer scenes.

**ServerScene**: The complete scene content required by the authoritative
server, including entities and their colliders.

**AgentScene**: The complete scene content required by an autonomous
participant, including entities and their colliders without visual or audio
assets.

**ClientScene**: The complete scene content required by a human participant's
client, including entities, their colliders and associated presentation content.

**SceneAsset**: The immutable compiled spatial scene from which independent live
scene instances are created.

**AssetId**: The identity of compiled content, shared by consumers using the
same resource definition and distinct from a content build's provenance.

**SceneEntityId**: The authored identity of a scene entity within one scene,
preserved independently of its author's prim naming and its live instance
identities. _Avoid_: PrototypeId

**SceneEntityDescription**: The immutable cooked description of one scene
entity, including its SceneEntityId, placement and available authored
components. It is not live mutable ECS state. _Avoid_: Prototype, scene recipe

**SceneInstance**: One independently placed and independently unloadable live
realization of a SceneAsset, owning its current members.

**LocalTransform**: An entity's authoritative placement relative to its parent;
for an entity without a parent, its placement in the world.

**WorldTransform**: An entity's derived placement in world coordinates.

**LocalBounds / WorldBounds**: Local and world-space visual culling envelopes,
respectively; they do not define collision or damage geometry.
