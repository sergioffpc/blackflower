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
11. `UpdateBotPerception` builds visual and acoustic perception from sealed
    state;
12. `PlanBotTactics` updates bot objectives, tactical plans, and navigation
    paths;
13. `EmitBotControlFrames` queues canonical bot controls for a future tick;
14. `SubmitTickOutputs` builds the sealed output batch, attaches a snapshot when
    due, and submits it to in-memory consumers.

The cadence policy defines 60 Hz control frames every 4 ticks, 30 Hz snapshots
every 8 ticks, 5 Hz bot perception and tactical updates every 48 ticks, and a
one-second input timeout after 240 ticks.

The pipeline performs no network I/O, serialization, socket access, file
handling, or wall-clock pacing.
