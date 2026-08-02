# blackflower-world-simulation

`blackflower-world-simulation` turns canonical in-memory actor inputs into sealed
authoritative state and in-memory tick outputs at 240 Hz.

Every fixed-step tick advances the ordered `SimulationPipeline` through these
phases:

1. `PrepareTick` opens the tick and activates scheduled commits;
2. `CaptureTickInputs` captures a canonical immutable input set;
3. `DeriveActorActions` derives deterministic actor actions;
4. `SolveRigidBodyDynamics` advances characters, rigid bodies, constraints, and
   collision response;
5. `SolvePhysicalPhenomena` advances ballistics, material responses,
   explosions, fire, and smoke;
6. `SolveAcoustics` propagates sound and builds acoustic observations;
7. `DeriveStateTransitions` derives canonical discrete-transition candidates;
8. `CommitStateTransitions` resolves conflicts and applies accepted transitions
   once;
9. `UpdateSpatialStructures` updates and versions collision, navigation,
   acoustic, and visibility structures;
10. `SealTick` validates, hashes, and seals the authoritative state;
11. `SubmitTickOutputs` builds the sealed output batch, attaches a snapshot when
    due, and submits it to in-memory consumers.

The cadence policy defines 60 Hz client control frames every 4 ticks, 30 Hz
snapshots every 8 ticks, a 12-tick input grace followed by neutral control, and
a separate one-second failsafe after 240 ticks.

When installed, `AcousticWorld` backs the five `SolveAcoustics` systems and
`UpdateAcousticStructure`. It resolves bounded zone/portal candidates, direct
and transmitted geometry, path arrivals on a 48 kHz timeline, masking,
privacy-preserving observations, and gated client deliveries. Committed door,
destructible, and portal changes become visible on the next tick.

The pipeline performs no network I/O, serialization, socket access, file
handling, or wall-clock pacing.

Human and bot participants both arrive as ordinary client inputs. Bot
perception, memory, planning, navigation, and control generation run outside
the authoritative world against snapshots and client-facing events.
