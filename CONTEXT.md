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

**CollisionShape**: A geometric shape that defines a surface or volume for
collision with participants or objects, independently of its visual
representation.

**Scene entity**: An individually identifiable entity placed in a scene, with an
associated visual representation and optional collision shapes. Its placement
does not imply that it can never move or be destroyed.

**Entity definition**: A reusable description of a scene entity's visual
representation and collision shapes, shared by its independently placed
instances.

**Scene**: The spatial description of an environment, including placed entities,
their visual and collision representations, lights and spawn points. It is
independent of exercise objectives and gameplay rules.

**Scenario**: A training exercise definition that includes objectives. It is
distinct from the spatial description provided by a Scene; its detailed contract
remains to be defined.

**Cooked content**: Scenario resources prepared before deployment in the
representation required by the runtime, rather than source assets awaiting
conversion.

**Content pack**: The signed distribution artifact containing a coherent set of
cooked scenario resources and their compatibility and integrity metadata.

**ContentBuildId**: The shared identity of a produced set of cooked content,
covering its source, production settings and resources across consumer scenes.

**ServerScene**: The complete scenario content required by the authoritative
server, including collision data and participant placement information.

**AgentScene**: The complete scenario content required by an autonomous
participant, including collision data without visual or audio assets.

**ClientScene**: The complete scenario content required by a human participant's
client, including collision, visual and audio data. Spawn placement is owned by
the server.
