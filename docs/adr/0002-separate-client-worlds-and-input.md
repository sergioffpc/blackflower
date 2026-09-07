# Separate client prediction and presentation with interchangeable input

Status: accepted for the two-world separation, common input interface, GPU client physics, and static-only prediction, explicitly requested by the owner. The integration details below are proposed and unimplemented.

Each client has a Prediction World and a Presentation World. Human and autonomous-agent input sources feed the same command interface, prediction, and network path; changing the source does not select different simulation rules. This replaces the earlier presentation-only client proposal, preserving the server as the owner of official state and outcomes.

## Integration proposal

Use separate Flecs worlds with project-owned identifiers and value snapshots between them. Use GPU PhysX for prediction of the locally controlled player against static geometry only, as required by the owner. Dynamic-world state, including other players, comes from the server and is not locally simulated or used for predicted collision response. Share movement rules and geometry with the server, while keeping separate state and physics instances. The Presentation World derives local predicted poses, remote interpolation, camera state, and confirmed feedback without writing back into prediction.

One client command pipeline sequences inputs, retains a bounded history, advances prediction, and sends the same commands to the server. Authoritative snapshots include a server tick and the last resolved input sequence; reconciliation restores the corresponding authoritative baseline and replays only unresolved movement inputs. Discrete commands and confirmed effects have stable identifiers: replay must not send another shot or play another hit sound.

An agent may consume an immutable observation from client-visible state and emit the same commands as a human adapter. It cannot mutate either world or access private server state. The observation schema and agent policy remain open; this decision does not define an AI framework, agent intelligence, or a headless deployment. Both client modes retain the same world structure and presentation path. Window focus gates human input only; an autonomous source follows its own explicit start/stop lifecycle.

## Alternatives and consequences

- A presentation-only client would avoid prediction history and correction work but would not meet the owner's requested separation.
- Combining prediction and presentation in one mutable world would reduce state transfer but couple visual smoothing to simulation and agent observations.
- Separate human and agent simulation paths would duplicate rules and undermine input-source interchangeability.

This choice adds Windows GPU PhysX cross-build/runtime work, prediction history, and reconciliation tests to the MVP architecture. It does not assume identical floating-point physics across Linux and Windows; authoritative correction remains mandatory. Precise replay state, history limits, correction tolerances, and observation contents must be resolved before implementing their delivery slices. Existing MVP performance targets still apply to four rendering clients.

The server remains authoritative for static collisions as well as dynamic ones. A local prediction may temporarily disagree at player contact; apply the server correction instead of extrapolating dynamic bodies or resolving that contact locally. The local player's provisional pose is the explicit exception to otherwise server-supplied dynamic state.

GPU use is a requirement, not a demonstrated implementation. In the inspected PhysX 5.5 documentation, scene queries run on CPU and GPU rigid-body support does not migrate the standard character controller to GPU. Prove a GPU rigid-body movement approach that preserves capsule geometry, speed, wall sliding, and correction behavior, or revisit the specific integration with the owner. Merely enabling a GPU scene around CPU controller queries is not evidence of GPU prediction. See [GPU simulation](https://nvidia-omniverse.github.io/PhysX/physx/5.5.0/docs/GPURigidBodies.html) and [character controllers](https://nvidia-omniverse.github.io/PhysX/physx/5.5.0/docs/CharacterControllers.html).

Integration clarification: [ADR-0004](0004-keep-io-outside-ecs.md) requires all external operations, including physics and agent invocation, to run outside ECS execution. Worlds exchange prepared input, physics requests/results, and output values with application orchestration; references above to the command or movement pipeline do not authorize I/O from a Flecs system.
