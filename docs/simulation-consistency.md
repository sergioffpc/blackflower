# Simulation consistency policy

Blackflower uses a server-authoritative simulation with local client prediction.
ARM64 clients and x86-64 servers are supported participants in the same session.
The client is not expected to reproduce the server state bit for bit; snapshots
correct prediction when gameplay-owned comparison tolerances are exceeded.

This policy separates three different requirements that must not be conflated:

- authoritative server rollback and replay must be repeatable within one
  certified server build and runtime configuration;
- forward prediction and reconciliation re-simulation must execute the same
  prediction implementation;
- client/server comparison across architectures is approximate for continuous
  values and exact only for discrete or canonical values.

## Unified prediction driver

Client gameplay that participates in prediction implements
`blackflower_world_prediction::PredictionDriver` once. Forward prediction and
reconciliation call its single `simulate_tick` entry point with an explicit
`PredictionPass`. Separate forward and re-simulation implementations are
forbidden.

The authoritative server continues to own `SimulationWorld`. There is no
abstract simulation-core crate: a shared gameplay crate should be introduced
only when concrete state, input, and gameplay systems genuinely have the same
implementation on the server and client.

The simulation rate is exactly 240 Hz. The ECS and Jolt boundaries receive the
pinned binary32 value `0x3b888889`; the public physics API does not accept an
arbitrary time step.

## Cross-platform reconciliation

`PredictionCodec::compare_states` owns the comparison of the predicted subset.
It returns `WithinTolerance` only when every field satisfies its policy:

- entity IDs, enums, counters, inventory, ammunition, and other discrete state
  compare exactly;
- positions, velocities, angles, and other continuous fields use explicit
  tolerances selected in their domain units;
- invalid or non-finite values always require correction;
- canonical network values may compare in their quantized representation when
  that representation is the gameplay contract.

`AbsoluteTolerance` and `AngularTolerance` are primitives for codec authors,
not global engine epsilons. Tolerances must reflect gameplay observability,
network quantization error, and accumulated prediction error. Approximate
equality is not transitive, so it must never implement `Eq`, define map keys,
determine canonical ordering, or directly define a state hash.

Hashes, replication digests, and persistent canonical state use explicit
quantized representations. Branches near gameplay thresholds must use stable
quantization or hysteresis where repeated cross-platform decisions matter.

## Compatibility identity

`SimulationCompatibilityId` identifies the authoritative gameplay rules,
protocol contract, canonical quantization/schema choices, and solver-policy
revision. Required cooked content remains independently identified by
`RequiredContentSetId`. CPU architecture, SIMD path, and client floating-point
bit patterns do not participate in session admission. They may be recorded as
telemetry and used to certify server deployment configurations.

Jolt is compiled without fast-math or floating-point contraction and with its
cross-platform deterministic option enabled. These controls reduce numerical
drift and support repeatable rollback within a server configuration; they are
not a promise of byte-identical ARM64 and x86-64 client/server state. Contact
facts are canonically ordered before entering gameplay state.

## Scripting and animation

Luau and ozz-animation are excluded from the authoritative simulation and
prediction dependency trees. Animation may consume sealed simulation state but
cannot contribute root motion to authoritative state. Scripts may propose
validated intents outside the tick; they cannot execute inside the shared
gameplay step. Luau native codegen is never an authoritative mode.

Each Luau evaluation receives a fresh global environment and a host-derived
seed before bytecode loading. This makes isolated evaluations reproducible but
does not make Luau an admissible authoritative numeric backend.

## Tick failure semantics

Flecs progress is not transactional: a callback can mutate ECS state before a
later callback fails. A failed simulation tick therefore makes that world
terminally faulted. The last completed tick counter is retained only for
diagnostics; the ECS state must not be hashed, replicated, or advanced again.
The owner must discard the authoritative world. Prediction may resume only
after restoring the complete predicted component set from an authoritative
snapshot and explicitly resetting its timeline.

This fail-stop rule prevents partially mutated state from being treated as a
successful rollback. True tick atomicity requires a bounded state journal or
pre-tick snapshot and remains a prerequisite before recoverable in-process tick
failures can be supported.

## Enforcement

`scripts/check-simulation-policy.sh` rejects scripting and animation in the
authoritative world and prediction dependency trees. CI runs this check
alongside formatting, Clippy, and tests. Floating-point gameplay is permitted
under the comparison and canonicalization rules above. Format and protocol
versions remain unchanged.
