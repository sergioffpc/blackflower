# blackflower-world-prediction

`blackflower-world-prediction` advances fixed-step client-side prediction at
240 Hz and reconciles it with authoritative server snapshots. ARM64 clients and
x86-64 servers are allowed in the same session; continuous state is compared
with gameplay-owned tolerances instead of requiring bit-identical results.

Every predicted tick uses the same `PredictionPipeline` in either
`PredictionPass::Forward` or `PredictionPass::Resimulation`. Its phases are:

1. `PrepareTick` prepares the fixed-tick context, resets transient storage, and
   activates scheduled state;
2. `CaptureTickInputs` captures the exact `InputFrame` already selected by the
   shared prediction coordinator;
3. `DeriveActorActions` derives deterministic actor actions;
4. `SolveRigidBodyDynamics` solves the locally predicted subset of rigid-body
   dynamics;
5. `DeriveStateTransitions` derives speculative state transitions;
6. `CommitStateTransitions` commits accepted transitions once;
7. `SealTick` validates the state and canonicalizes stable predicted-event IDs;
8. `SubmitTickOutputs` classifies and reconciles forward, replayed, corrected,
   and cancelled events before idempotent in-memory publication.

The authoritative reconciliation flow is:

1. find the locally predicted state for the authoritative snapshot tick;
2. compare discrete fields exactly and continuous fields with explicit
   domain-specific tolerances;
3. if the state is within policy, discard obsolete prediction and input history;
4. if correction is required, validate the re-simulation bound and every
   required input before mutating the prediction world;
5. restore the authoritative state, replace the prediction history at that
   tick, and discard later predicted states;
6. run the same prediction pipeline in `Resimulation` for each subsequent
   recorded input, rebuilding prediction history up to the previous local tick;
7. require a hard resync instead of partial re-simulation when the snapshot,
   predicted state, input history, or configured work bound is insufficient.

The crate owns no wall-clock pacing, device input sampling, network I/O,
snapshot decoding, concrete simulation state, or presentation.
