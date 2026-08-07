# blackflower-world-simulation

`blackflower-world-simulation` turns canonical in-memory actor inputs into sealed
authoritative state and in-memory tick outputs at 240 Hz.

The initial concrete gameplay slice owns controllable actor movement and
absolute view orientation. A canonical control covers four ticks, drives a
fixed five-metre-per-second flat-ground controller, retains the last control for
the twelve-tick grace window, and then neutralizes movement while preserving
orientation. Sealed actor state exposes transform, velocity, grounded state,
and the latest applied input identity through a transport-neutral frame.

Every fixed-step tick advances the ordered `SimulationPipeline` through these
phases:

1. `PrepareTick` opens the tick and activates scheduled commits;
2. `CaptureTickInputs` captures a canonical immutable input set;
3. `DeriveActorActions` derives deterministic actor actions;
4. `ResolveHistoricalCommands` resolves bounded read-only rewind and projectile
   catch-up commands into current-tick facts;
5. `SolveRigidBodyDynamics` advances characters, rigid bodies, constraints, and
   collision response;
6. `SolvePhysicalPhenomena` advances ballistics, material responses,
   explosions, fire, and smoke;
7. `SolveAcoustics` propagates sound and builds acoustic observations;
8. `DeriveStateTransitions` derives canonical discrete-transition candidates;
9. `CommitStateTransitions` resolves conflicts and applies accepted transitions
   once;
10. `UpdateSpatialStructures` updates collision, acoustic, and authoritative
    visibility structures and publishes navigation traversability changes;
11. `SealTick` validates, canonicalizes events, hashes, and seals the
    authoritative state;
12. `SubmitTickOutputs` builds one tick-keyed sealed output batch, including
    command dispositions and a transport-neutral replication view when due.

The cadence policy defines 60 Hz client control frames every 4 ticks, 30 Hz
snapshots every 8 ticks, a 12-tick input grace followed by neutral control, and
a separate one-second failsafe after 240 ticks.

`SimulationWorldConfig` explicitly selects disabled or required authoritative
acoustics. In required mode a missing `AcousticWorld` fails the tick rather than
silently producing no facts. When installed, `AcousticWorld` backs the five `SolveAcoustics` systems and
`UpdateAcousticStructure`. It resolves bounded zone/portal candidates, direct
and transmitted geometry, path arrivals on a 48 kHz timeline, masking,
privacy-preserving observations, and gated client deliveries. Committed door,
destructible, and portal changes become visible on the next tick.

The pipeline performs no network I/O, serialization, socket access, file
handling, or wall-clock pacing.

Human and bot participants both arrive as ordinary client inputs. Bot
perception, memory, planning, navigation, and control generation run outside
the authoritative world against snapshots and client-facing events. The
simulation publishes navigation changes but never owns a bot Detour runtime.
