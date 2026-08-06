# Blackflower autonomous agents v1

Status: normative design contract for the human-like bot agent, revision 1.

The keywords **MUST**, **MUST NOT**, **SHOULD**, and **MAY** are normative. An
"agent" is a non-human participant that plays a match through the same interface
as a human. A "human trace" is a recorded stream of (perception, input) pairs
captured from a consenting human player. "Behavioral cloning" (BC) is supervised
imitation of human traces. A "decision record" is a bounded, immutable diagnostic
projection of one decision that has already been made; it is evidence of the
inputs, alternatives, constraints, and output involved, not an input to the
decision. This document governs where agents live, what they are allowed to
perceive, how a neural-network (NN) policy may drive them without granting any
advantage over a human, and how an operator can inspect their behavior.

## Scope and ownership

- **BOT-ENV-001**: v1 MUST deliver opponents and teammates that a human player
  cannot reliably distinguish from another human within a single match, at the
  same match scale as networking (one dedicated regional server, at most 32
  participants including agents).
- **BOT-ENV-002**: a new crate `blackflower-agent` MUST own perception encoding,
  the behavior/decision model, NN inference, and the human-likeness (reaction,
  aim, movement) models. It MUST depend on `blackflower-harness`,
  `blackflower-navigation`, and `blackflower-spatial-query`.
- **BOT-ENV-003**: `blackflower-agent` MUST NOT depend on `blackflower-ecs`,
  `blackflower-world-simulation`, `blackflower-networking-replication`, or any
  authoritative world state type. Fairness is enforced by the dependency graph,
  not by convention.
- **BOT-ENV-004**: agent code MUST NOT define gameplay rules. It observes and
  emits input only; the authoritative simulation remains the sole executor of
  commands, identically for humans and agents.
- **BOT-ENV-005**: the NN policy, its training pipeline, and the deterministic
  simulation core MUST remain separate concerns. Training tooling MUST NOT be
  linked into the shipping server or client.
- **BOT-ENV-006**: `blackflower-agent` MUST own typed agent metrics, log events,
  sensorium/decision diagnostic records, and their observer contracts, but MUST
  NOT install process-global observability backends or own a terminal UI. The
  executable hosting agent decision loops owns observability initialization and
  foreground UI composition.

## Agent interface and parity with a human

- **BOT-IFACE-001**: an agent MUST drive the match exclusively through the
  `blackflower-harness` client interface: it consumes the same replicated world
  view a human client receives and injects the same input frames.
- **BOT-IFACE-002**: an agent MUST emit only the command and input types defined
  for human players. It MUST NOT introduce privileged commands and MUST NOT
  bypass the command classification path.
- **BOT-IFACE-003**: an agent MUST NOT read authoritative ECS state, other
  clients' pending inputs, entity data outside its own replicated interest set,
  or any server-only structure. The only permitted world inputs are (a) the
  replicated snapshot delivered to its harness and (b) static cooked level data
  the human client also loads (geometry, navmesh, acoustic data).
- **BOT-IFACE-004**: an agent's input-submission cadence and per-frame command
  budget MUST NOT exceed what a human client is allowed. An agent MUST NOT
  achieve lower end-to-end action latency than the human command path permits.
- **BOT-IFACE-005**: agents SHOULD be runnable both as an out-of-process client
  over the real transport and as an in-process client that reuses the same
  harness observation/input API without the network hop. Both paths MUST expose
  the agent to exactly the same information; the in-process path MUST NOT be a
  shortcut to additional state.

## Perception

- **BOT-PERCEPT-001**: the replicated interest set is the outer fairness bound
  (an agent cannot receive more than a human). On top of it, the agent MUST
  apply its own sensory model and MUST NOT act on an entity it received but
  cannot sense.
- **BOT-PERCEPT-002**: vision MUST be gated by a field-of-view cone plus a
  line-of-sight test against static level geometry using
  `blackflower-spatial-query`. An entity outside the cone or occluded by
  geometry MUST NOT enter the agent's perceived set, even when present in the
  replicated snapshot.
- **BOT-PERCEPT-003**: line-of-sight tests MUST use an early-terminating
  occlusion query. The existing `blackflower-spatial-query::Scene::is_occluded`
  path backed by `rtcOccluded1` is the required primitive; the agent MUST NOT use
  the all-intersections path merely to answer an occlusion predicate.
- **BOT-PERCEPT-004**: hearing MUST be derived from the same audio events a
  human client can perceive, attenuated by distance and occlusion. Sound MUST
  yield only a direction/position estimate with uncertainty, never an exact
  entity identity or state.
- **BOT-PERCEPT-005**: the agent MUST maintain a memory of last-known
  observations with confidence decay over time, and MUST act on remembered
  belief rather than instantaneous ground truth. Reacquisition MUST clear stale
  belief.
- **BOT-PERCEPT-006**: perception MUST impose a reaction latency between an
  observation becoming available and the agent being permitted to act on it.
  The latency MUST be drawn from a per-agent distribution, not a constant.
- **BOT-PERCEPT-007**: perception MUST be encoded into a fixed-size feature
  representation (the policy observation) that contains only sensed information.
  Raw ECS fields, absolute entity identities of unsensed entities, and any
  server-only value MUST NOT appear in the observation vector.

## Player sensorium and performance state

External perception alone is not the complete information boundary of a human
player. The agent also needs the client-visible state of its own body, equipment,
and temporary impairments. This complete state is the `PlayerSensorium`:

```text
client-visible snapshots and events
  -> gameplay-owned PlayerSensorium
       -> human HUD / visual, audio, and haptic feedback
       -> foreground SensoriumSnapshot
       -> reaction gate -> AgentMemory / Belief -> BotObservationEncoder
                          -> foreground MemorySnapshot
```

- **BOT-SENSE-001**: gameplay MUST own one versioned `PlayerSensorium` semantic
  schema. Human presentation/HUD and the bot observation encoder MUST derive from
  that shared knowledge rather than independently interpreting canonical
  component bytes. Frontends MAY present a richer visual or acoustic rendering,
  but the bot representation MUST be semantically equivalent or weaker and MUST
  never contain a fact unavailable to the human player.
- **BOT-SENSE-002**: the schema MUST be capability-driven. Every channel carries
  availability, source tick or event identity, freshness, and uncertainty where
  applicable. A channel not implemented by gameplay or not delivered to a human
  client MUST be `unavailable`; consumers MUST NOT invent a neutral zero, healthy
  default, or synthetic estimate.
- **BOT-SENSE-003**: the external-vision channel MUST describe the exact semantic
  visual evidence admitted by BOT-PERCEPT: view direction/FOV, visibility
  conditions, and a bounded set of sensed stimuli with relative direction,
  distance band, salience, occlusion state, confidence, and age. Smoke, darkness,
  glare, flash, optics, and other visibility modifiers MUST appear only when the
  human rendering/feedback contract represents their effect.
- **BOT-SENSE-004**: the hearing channel MUST consume the existing
  `AcousticObservation` contract for bots. It MAY expose sound class, arrival age,
  received band energy, uncertain listener-relative direction, masking, and
  hearing response. It MUST NOT recover a source entity, exact source position,
  speaker identity, voice content, or emission ID.
- **BOT-SENSE-005**: proprioception, vestibular state, touch, and impact feedback
  MUST be represented when available to a human through direct control,
  animation, camera, HUD, audio, or haptics. Candidate fields include stance,
  grounded/support state, relative velocity and acceleration, balance, recoil,
  fall/landing state, surface contact, impact direction, and blast impulse. A hit
  cue MUST NOT reveal the attacker or exact origin unless the human feedback does.
- **BOT-SENSE-006**: internal physical condition MUST include every gameplay state
  a human can feel or inspect that can influence performance. Candidate
  capabilities include stamina, exertion/fatigue, breath or oxygen debt, pain,
  suppression/stress, blood loss, bleeding, consciousness, mobility, encumbrance,
  hydration/nutrition, ambient exposure, core/skin temperature, and localized
  injuries by body region and severity. This list does not declare those systems
  implemented; once gameplay exposes one to the human, omitting it from the bot
  sensorium is a parity failure.
- **BOT-SENSE-007**: equipment and action capacity MUST include human-visible
  state that gates or modifies input: equipped item, ammunition, reload/jam,
  cooldowns, heat, stance transition, interaction availability, carry weight,
  and disabled or degraded actions. The agent MUST receive the same semantic
  precision and update cadence as the human HUD, not hidden authoritative timers
  or internal durability values.
- **BOT-SENSE-008**: sensory impairments and status effects MUST be explicit
  channels rather than implicit policy guesses. Examples include blinded/dazzled,
  smoke exposure, deafened/tinnitus, pain shock, concussion/disorientation,
  hypothermia/hyperthermia, medication or toxin effects, and suppression. Each
  active effect MUST state its perceived severity, age/remaining coarse band, and
  which faculties it is permitted to modify.
- **BOT-SENSE-009**: gameplay MUST derive a `PerformanceEnvelope` from the exact
  modifiers applied to the agent controller. It MUST expose base, effective, and
  bounded ordered contributors for at least reaction latency, visual/hearing
  acuity, aim stability, turn rate, movement speed/acceleration, action
  availability, and decision cadence where those dimensions exist. Diagnostics
  MUST display this supplied derivation; they MUST NOT recompute it from raw bars.
- **BOT-SENSE-010**: the sensorium MUST preserve causal distinctions. For example,
  low stamina, a wounded leg, heavy equipment, and a stance transition may all
  reduce movement but MUST remain separate contributors. A single synthetic
  "health", "fitness", or "confidence" score MUST NOT replace the underlying
  human-perceivable state.
- **BOT-SENSE-011**: a bot MAY react only to the sensorium snapshot admitted by
  its reaction gate. A newer bodily or sensory change can be visible in live
  diagnostics while still marked `not_yet_reactable`; the policy observation and
  decision record MUST retain the older admitted value until its modeled human
  latency elapses.
- **BOT-SENSE-012**: thermal sensation is interoception and environmental
  exposure, not thermal vision. It MUST NOT reveal heat sources through geometry
  or grant infrared perception unless an equivalent human-visible device or
  effect is active.
- **BOT-SENSE-013**: semantic precision MUST match human feedback. If a human sees
  a stamina bar, the agent MAY receive its equivalent normalized value; if the
  human perceives only coarse pain, temperature, or injury bands, the agent MUST
  NOT receive the underlying exact authoritative scalar or expiry time.

### Perceptual, spatial, and working memory

The memory shown by diagnostics is the bot's actual bounded runtime memory, not a
history reconstructed from authoritative state after the fact.

- **BOT-MEM-001**: `blackflower-agent` MUST own an explicit semantic
  `AgentMemory` separate from `PlayerSensorium`, the authoritative snapshot
  window, and any opaque NN recurrent state. The policy MUST read this same
  memory; a diagnostics-only shadow model is forbidden.
- **BOT-MEM-002**: memory MUST distinguish at least sensory memory (recent visual,
  acoustic, touch, and impact observations), spatial belief (last-known or
  inferred regions), episodic memory (bounded recent events and outcomes), and
  working memory (current goal, plan, target token, and unresolved task).
- **BOT-MEM-003**: every memory item MUST carry a process-local non-reversible
  token, kind, originating modality, first/last-observed ticks, last-known
  relative region or direction, confidence, uncertainty, decay/expiry policy,
  current status, and links to the observations and decisions that created or
  consumed it. It MUST NOT retain an authoritative entity ID.
- **BOT-MEM-004**: an unsensed entity or event MUST NOT move inside memory by
  reading its newer replicated or authoritative state. A gameplay-defined
  prediction MAY extrapolate from the last legal observation, but it MUST be
  marked `inferred`, grow uncertainty with time, and stop at its bounded horizon.
- **BOT-MEM-005**: confidence decay, uncertainty growth, expiry, capacity, and
  eviction MUST be explicit bounded policies. Difficulty MAY tune realistic human
  memory quality but MUST NOT disable decay, grant perfect recall, or extend the
  legal observation boundary.
- **BOT-MEM-006**: new legal evidence MUST produce explicit transitions such as
  `corroborated`, `contradicted`, `reacquired`, `expired`, or `forgotten`.
  Reacquisition MUST replace stale spatial belief with the new sensed evidence;
  contradiction MUST remain visible in bounded history instead of silently
  rewriting the earlier memory.
- **BOT-MEM-007**: the runtime MUST distinguish `observed` from
  `reaction_admitted`. A fresh perception MAY enter sensory memory immediately,
  while the policy and working memory continue using the previously admitted
  value until BOT-PERCEPT-006 permits reaction.
- **BOT-MEM-008**: memory MUST reset or rebind explicitly on controlled-entity
  changes, episode/life boundaries, map/content identity changes, and hard resyncs
  that invalidate its observation timeline. Cross-match online memory and
  learning remain forbidden by v1.
- **BOT-MEM-009**: static cooked map/navigation knowledge, fixed policy weights,
  and authored tactics are long-term prior knowledge, not episodic memory. The UI
  MUST render them as background/prior context and MUST NOT imply the bot learned
  them during the current match.
- **BOT-MEM-010**: an RNN/LSTM/transformer cache MAY supplement semantic memory,
  but its hidden tensor MUST NOT be translated into invented human-readable
  memories. Diagnostics MAY show its version, shape, age, reset reason, numerical
  health, and whether it was used; decision explanations MUST still link to the
  explicit semantic memory and legal policy inputs.
- **BOT-MEM-011**: foreground diagnostics MUST receive a bounded immutable
  `MemorySnapshot`; the UI MUST NOT borrow or lock live mutable `AgentMemory`.
  Snapshot production uses the optional diagnostic observer and MUST perform no
  clone, allocation, or queue operation while no observer is installed.

### Current capability posture (non-normative)

At this proposal revision, the workspace provides foundations but not a complete
player physiology model:

| Channel                | Current evidence                                                                                                           | Remaining work                                                                                     |
| ---------------------- | -------------------------------------------------------------------------------------------------------------------------- | -------------------------------------------------------------------------------------------------- |
| Vision                 | `Scene::is_occluded` is backed by Embree `rtcOccluded1`                                                                    | Gameplay FOV, visual-stimulus schema, salience, and visibility modifiers                           |
| Hearing                | `AcousticObservation` already carries privacy-preserving class, arrival, band energy, uncertain direction, and uncertainty | Agent memory/masking integration and sensorium adapter                                             |
| Memory                 | BOT-PERCEPT defines the required last-known-observation behavior                                                           | No implemented semantic memory store, decay policy, or diagnostic snapshot exists yet              |
| Proprioception         | Simulation/presentation pipelines name character transform, velocity, support, camera, and listener stages                 | Gameplay-owned semantic DTO and actual populated providers                                         |
| Haptics and HUD        | Presentation has named haptic, UI, and semantic-visibility phases                                                          | These are structural callbacks, not implemented feedback contracts                                 |
| Stamina and physiology | Prediction tolerances can support gameplay-defined fields such as stamina                                                  | No concrete stamina, fatigue, breath, pain, bleeding, injury, or consciousness contract exists yet |
| Thermal state          | Simulation scaffolding names thermal/exposure phenomena                                                                    | No player temperature, exposure, or thermal-sensation contract exists yet                          |

## Navigation

- **BOT-NAV-001**: navigable space MUST be queried through
  `blackflower-navigation` (recast/detour). The agent MUST NOT invent
  traversability outside the cooked navmesh.
- **BOT-NAV-002**: path following MUST use local avoidance so that multiple
  agents do not intersect or oscillate. V1 MAY satisfy this through bounded
  local steering plus replanning over the existing static Detour surface;
  exposing `DetourCrowd` or `DetourTileCache` is not a v1 prerequisite.
- **BOT-NAV-003**: path requests MUST be issued only when a goal changes, a path
  becomes invalid, or the replanning policy expires; they MUST NOT run
  unconditionally for every agent on every decision. `NavMesh` and `Query` MUST
  stay on their owning navigation worker because they are neither `Send` nor
  `Sync`; that worker MUST reuse query scratch and return bounded DTO results.
  Requests MUST be budgeted and MAY be amortized across ticks.
- **BOT-NAV-004**: movement output MUST be expressed as the same movement input
  a human produces (direction/intent), never as a teleport or a direct pose
  write.

## Neural-network policy

- **BOT-NN-001**: when an NN policy is used to make the agent human-like, it MUST
  be trained by behavioral cloning from human traces. Reinforcement learning
  MUST NOT be the human-likeness mechanism, because reward-optimized policies
  converge to superhuman, non-human behavior. RL MAY be used only for an
  explicitly labeled "challenge" difficulty that is not claimed to be human.
- **BOT-NN-002**: the policy input MUST be exactly the perception observation
  vector of BOT-PERCEPT-007, including the admitted `PlayerSensorium` fields. The
  policy output MUST map onto the human command surface (movement intent, aim
  delta, discrete actions), and MUST NOT express any command a human cannot.
- **BOT-NN-003**: inference MUST run through an in-engine runtime with no network
  dependency and no dynamic code loading beyond the model weights (e.g. `ort`
  or `tract` consuming ONNX). Cloud or API-hosted model calls MUST NOT be on any
  per-tick or per-decision path.
- **BOT-NN-004**: a single inference MUST complete within the agent decision
  budget of BOT-PERF-002. Model size and cadence MUST be chosen to satisfy that
  budget for the full agent count of BOT-ENV-001.
- **BOT-NN-005**: model weights are assets and MUST be delivered through the
  signed asset pipeline (`blackflower-assets`), so provenance is verified before
  load. Unsigned or unverified weights MUST NOT be loaded on an authoritative
  server.
- **BOT-NN-006**: the agent MUST remain fully functional with the NN policy
  disabled, falling back to the classical behavior model of BOT-REAL. NN is an
  enhancement layer, not a hard dependency of shipping.
- **BOT-NN-007**: the NN MAY be scoped to specific faculties (e.g. aim
  trajectory, short-horizon movement) while a classical planner owns high-level
  tactics, provided every faculty still obeys BOT-NN-002.

## Determinism and authority

- **BOT-DET-001**: agent-controlled gameplay entities MUST be simulated only on
  the authoritative server. The decision process MAY be hosted in-process or
  out-of-process as allowed by BOT-IFACE-005, but its outputs enter simulation as
  authoritative inputs exactly like a remote human's inputs.
- **BOT-DET-002**: agent decision-making and NN inference MUST NOT execute inside
  prediction, rollback, or resimulation. An agent's ordinary client harness MAY
  maintain the same predicted view as a human client, but a reconciliation MUST
  NOT rerun or rewrite past decisions; only accepted `ControlSubmission` values
  cross into the authoritative path. NN inference is not bit-reproducible across
  hardware and MUST remain outside the deterministic tick core.
- **BOT-DET-003**: because agent decision-making is excluded from the
  deterministic core, it MAY use ordinary floating point and MUST NOT be
  required to satisfy the cross-platform determinism policy that governs the
  simulation. It MUST NOT, however, mutate any state read by that core except
  through the sanctioned input channel.

## Human-likeness models (classical baseline)

- **BOT-REAL-001**: the agent MUST implement, independent of any NN, a reaction
  model, an aim model, and a movement model. These carry the human-likeness
  when NN is disabled and constrain the NN output when enabled.
- **BOT-REAL-002**: aim MUST be modeled as latency-delayed target tracking with
  an acquisition phase, a settle phase, error that decreases with time-on-target,
  occasional overshoot with correction, and a small constant tremor. Instant
  snapping and zero-error aim are forbidden.
- **BOT-REAL-003**: movement MUST include micro-corrections, hesitation, cover
  usage, and imperfect strafe timing. Perfectly straight paths and frame-perfect
  strafing are forbidden.
- **BOT-REAL-004**: difficulty MUST be expressed only by tuning reaction latency,
  aim error, and decision quality. Difficulty MUST NOT be implemented by
  granting the agent additional perception or reduced input latency.
- **BOT-REAL-005**: agents MUST make occasional, plausible mistakes (misjudged
  peeks, wrong sound reads, reload at a bad moment). A mistake-free agent fails
  the human-likeness goal.

## Data and training

- **BOT-DATA-001**: human traces MUST be captured only with explicit player
  consent and MUST record only (perception observation, emitted input) pairs at
  the agent's information boundary, never privileged server state.
- **BOT-DATA-002**: captured traces are personal data and MUST follow the
  observability redaction posture; player identity MUST be pseudonymized in the
  stored dataset.
- **BOT-DATA-003**: training runs offline outside this workspace (e.g. PyTorch or
  JAX) and MUST export to ONNX. The training environment MAY reuse the headless
  server as a data source or evaluator, but training code MUST NOT ship.
- **BOT-DATA-004**: every shipped model MUST record its dataset revision,
  training config hash, sensorium/feature-vector schema versions, and engine
  compatibility, so a schema change invalidates incompatible models rather than
  silently misfeeding them.

## Trace capture format

- **BOT-TRACE-001**: human capture MUST occur at the harness input boundary
  through a proposed `TraceObserver` hook, invoked exactly once per accepted
  submission (after validation, sequencing, prediction queueing, and transport
  publish). It MUST NOT read predicted, server-only, or ECS state. Capturing
  anywhere else risks recording information a fair player never had.
- **BOT-TRACE-002**: each record MUST pair the submitted control (the label)
  with the authoritative projection window the client held (the observation
  source). Feature vectors MUST be derived offline by the shared perception
  encoder and MUST NOT be stored pre-derived, so an encoder change reprocesses
  the corpus instead of invalidating it (this is why BOT-DATA-004 versions the
  sensorium and feature-vector schemas).
- **BOT-TRACE-003**: the on-disk format MUST be explicit and little-endian with
  a versioned self-describing header. It MUST NOT use `serde`, `bincode`, host
  byte order, or Rust memory layout, mirroring NET-WIRE-001. Reusing the
  networking `Snapshot` canonical encoding for the observation payload is
  RECOMMENDED so capture and live decode share one codec.
- **BOT-TRACE-004**: episodes MUST be delimited. The recorder MUST mark session
  start, every control-binding change (new controlled entity), and session end,
  so training can segment by life/round and drop cross-episode transitions.
- **BOT-TRACE-005**: capture MUST be opt-in per player, the stored player
  identity MUST be pseudonymized, and only boundary data may be persisted
  (BOT-DATA-001, BOT-DATA-002).
- **BOT-TRACE-006**: the sink MUST NOT block gameplay. It buffers and flushes
  off the submit path and, under backpressure, drops records with a counted
  metric rather than stalling the client. Recording MUST impose no cost when no
  sink is attached.
- **BOT-TRACE-007**: a file MUST contain enough to reconstruct perception
  offline: the authoritative snapshot each record observed (inline or by
  reference into a snapshot stream in the same file), the submission, its input
  sequence, and the simulation tick.

### Record fields (per accepted submission)

The proposed `TraceRecord` handed to the sink at the tee carries exactly:

| Field            | Source                          | Role                              |
| ---------------- | ------------------------------- | --------------------------------- |
| `session_state`  | `ClientSession`                 | Filter to `Active` episodes       |
| `input_sequence` | Assigned by harness             | Correlate with server disposition |
| `authoritative`  | Newest reconstructed `Snapshot` | Observation source                |
| `window`         | Bounded snapshot history        | Motion/velocity derivation        |
| `submission`     | Canonical `ControlSubmission`   | Label (move/aim/buttons)          |

### File layout (non-normative)

```text
Header
  magic "BFTR", format_version:u16, sensorium_schema_version:u16,
  feature_vector_version:u16, build_id:[u8;32], map_id:u64,
  player_pseudonym:[u8;16], skill_tier:u8, sim_tick_rate:u32
Frame*  (little-endian, length-prefixed, zstd per block)
  kind:u8  (0=EpisodeStart 1=Observation+Action 2=BindingChange 3=EpisodeEnd)
  sim_tick:u64
  input_sequence:u64
  observation_len:u32, observation_bytes  (Snapshot canonical encoding)
  submission_len:u32, submission_bytes    (execute_tick, payload, commands)
```

The concrete recorder (framing, zstd, rotation) MUST live outside
`blackflower-harness`; the harness owns only the optional borrowed observer hook
so it remains dependency-light. Offline, the training pipeline reads each file,
runs the same gameplay-owned sensorium/perception encoder used at inference,
pairs the resulting feature vector with the decoded submission, segments by
episode, and exports supervised pairs for behavioral cloning (BOT-NN-001).

## Performance and scale

- **BOT-PERF-001**: agent decision-making MUST run at a bounded rate well below
  the 240 Hz simulation (target 10-30 Hz), decoupled from the fixed tick, with
  motor output (aim/move) smoothed into canonical 60 Hz control frames, each
  covering four simulation ticks.
- **BOT-PERF-002**: the process hosting decision loops MUST enforce a shared CPU
  and wall-time budget for all agents, time-slicing decisions across agents. An
  in-process host MUST schedule that work outside the authoritative 240 Hz tick
  and MUST NOT let agent budget exhaustion block simulation. The budget MUST hold
  for the full agent count of BOT-ENV-001.
- **BOT-PERF-003**: agents SHOULD apply level-of-detail by relevance: agents far
  from any human decide less frequently. LOD MUST NOT change what an agent is
  allowed to perceive, only how often it thinks.
- **BOT-PERF-004**: in-process agents MUST share the server without going over
  the real transport, while still routing through the same harness
  observation/input API (BOT-IFACE-005).

## Decision evidence

The operator-facing answer to "why did the bot do that?" is the actual bounded
decision chain, not a narrative reconstructed after the fact:

```text
ClientView
  -> PlayerSensorium
  -> reaction-admitted BotObservation
  -> AgentMemory, belief, and current intent
  -> bounded candidate evaluation or NN outputs
  -> selected semantic BotAction
  -> navigation, performance, and human-likeness constraints
  -> ControlSubmission
```

- **BOT-EXPLAIN-001**: every completed decision tick MUST be capable of producing
  one `DecisionRecord` from the exact intermediate values used by that decision.
  The record MUST NOT rerun perception or policy code, infer a reason from the
  emitted input, or generate a plausible natural-language explanation after the
  decision.
- **BOT-EXPLAIN-002**: a record MUST distinguish (a) what the agent sensed, (b)
  which bodily/equipment state affected performance, (c) what it remembered or
  believed and whether that memory was observed or inferred, (d) which semantic
  options were considered, (e) which option the policy selected, (f) which
  downstream constraints changed it, and (g) which control was accepted by the
  harness.
- **BOT-EXPLAIN-003**: records MUST be based only on information already legal
  under BOT-IFACE, BOT-PERCEPT, and BOT-SENSE. Diagnostics MUST NOT reattach
  unsensed snapshot entities, authoritative state, hidden player identity, or
  exact values deliberately made uncertain or coarse for the human.
- **BOT-EXPLAIN-004**: classical decisions MUST expose a bounded list of the
  highest-ranked semantic candidates with their component scores and rejection
  reasons. NN decisions MUST expose the bounded top semantic outputs and their
  raw model scores or calibrated probabilities, plus the selected output. A
  model score MUST NOT be presented as causal proof or as calibrated confidence
  unless the model contract defines and validates that calibration.
- **BOT-EXPLAIN-005**: reason, rejection, constraint, fallback, navigation,
  sensorium status, and policy-source fields MUST use stable bounded enums owned
  by the layer that makes the choice. Free-form text MAY add operator context but
  MUST NOT be the only machine-readable explanation.
- **BOT-EXPLAIN-006**: each agent MUST assign a monotonically increasing local
  decision sequence. When the selected action is submitted, its decision record
  MUST be correlated with the harness input sequence and execute tick. Held plans
  MUST retain the decision sequence that created them so the operator can tell a
  new choice from continued motor execution.
- **BOT-EXPLAIN-007**: an NN policy is not required to produce a false
  human-readable rationale. V1 explainability for an opaque model consists of
  its legal input summary, output scores, selected semantic action, and every
  deterministic constraint or fallback applied afterwards. Saliency analysis,
  counterfactual inference, and generated prose are outside the live decision
  path.
- **BOT-EXPLAIN-008**: decision recording MUST be observation-only. Enabling,
  disabling, pausing, filtering, or dropping diagnostic records MUST NOT consume
  agent RNG, change policy state, alter scheduling, extend a plan lifetime, or
  affect the emitted `ControlSubmission`.
- **BOT-EXPLAIN-009**: the agent-side `DecisionObserver` MUST be distinct from the
  harness `TraceObserver`: the former explains bot decisions to live diagnostics,
  while the latter captures consented human observation/action pairs for offline
  training. No `DecisionRecord` allocation, clone, or queue operation MAY occur
  when no decision observer is installed.

### Decision record fields

The exact Rust types remain an implementation detail, but every record MUST carry
the following bounded semantic groups:

| Group            | Required evidence                                                                                                                                                   |
| ---------------- | ------------------------------------------------------------------------------------------------------------------------------------------------------------------- |
| Correlation      | Process-local pseudonymous agent ID, decision sequence, observation tick, sensorium/schema and policy/model versions, and input sequence/execute tick once accepted |
| Timing           | Snapshot age, sensorium freshness, reaction gate, decision/inference duration, budget status, and plan expiry                                                       |
| Sensorium        | Reference to the exact `SensoriumSnapshot` admitted to the policy and any newer values still gated by reaction latency                                              |
| Perception       | Modality counts and a bounded set of salient sensed stimuli with confidence/uncertainty and observation age                                                         |
| Body/performance | Active condition, injury, impairment, equipment, and ordered `PerformanceEnvelope` contributors that affected the action                                            |
| Memory           | Reference to the exact `MemorySnapshot`, consumed item tokens, observed/inferred status, confidence/uncertainty, and memory age                                     |
| Belief           | Current goal/intent, remembered target or region, evidence source, corroboration/contradiction state, and confidence                                                |
| Candidates       | Bounded top classical candidates with scores/rejections, or bounded top NN semantic outputs with model scores                                                       |
| Selection        | Chosen semantic action, policy source (`classical`, `nn`, `held`, or `fallback`), and stable reason code                                                            |
| Constraints      | Reaction hold, action validation, navigation/steering result, performance envelope, aim/movement humanization, and every clamp, replacement, or neutral fallback    |
| Emission         | Bounded summary of the final `ControlSubmission` and whether the harness accepted or rejected it                                                                    |

Full feature vectors, raw snapshots, complete paths, unbounded candidate lists,
and model tensors MUST NOT be copied into the live foreground record. They belong
in explicit offline diagnostic tooling, subject to the same fairness and
redaction boundary.

## Foreground agent diagnostics

- **BOT-FG-001**: every executable that hosts one or more agent decision loops
  MUST offer an optional `--foreground` mode. Headless operation remains the
  default. Foreground mode MUST require an interactive terminal and MUST restore
  it during both orderly exit and recoverable failure.
- **BOT-FG-002**: foreground mode MUST follow the visual and interaction contract
  of `blackflower-server --foreground`: Ratatui alternate screen, bounded local
  histories, `Tab`/`Shift+Tab` page navigation, numeric page shortcuts, `?` help,
  `q`/`Ctrl-C` exit, and the same structured-log level, regex, pause, follow,
  scrolling, and clear behavior. Reusable foreground infrastructure SHOULD be
  extracted instead of maintaining a second divergent implementation.
- **BOT-FG-003**: when agents are hosted by `blackflower-server`, agent pages MUST
  compose into that process's existing foreground dashboard. When they are
  hosted by a separate runner, that runner MUST expose the same diagnostics
  surfaces without opening a privileged connection to the authoritative world.
- **BOT-FG-004**: the foreground UI MUST provide at least these agent-specific
  surfaces:
  - **Agents**: bounded table of active agents, session state, difficulty,
    policy/model version, current intent, policy source, last-decision age,
    observation age, and health/fallback state;
  - **Sensorium**: visual live inspection of external senses, internal condition,
    equipment/action capacity, perceptual/working memory, belief, and their
    effective performance consequences;
  - **Decisions**: selectable per-agent chronological feed and a detail view that
    renders the decision-record groups in causal order;
  - **Overview integration**: aggregate active/healthy/stalled agent counts,
    decision and inference latency, budget pressure, fallbacks, and dropped
    diagnostic records.
- **BOT-FG-005**: the Decisions surface MUST make the selected semantic action
  and every downstream override visually distinct. It MUST be possible to answer
  whether a bot acted because of current vision, uncertain hearing, bodily
  condition, equipment state, a specific observed/inferred memory, goal
  continuation, an NN/classical policy choice, path replanning, a reaction gate,
  a human-likeness modifier, or a safety fallback without reading raw log prose.
- **BOT-FG-006**: metrics MUST be read from the process's loopback Prometheus HTTP
  endpoint, as the server foreground already does. A runner whose normal client
  configuration has no exporter MUST enable a loopback-only endpoint while
  foreground diagnostics are active; the UI MUST NOT inspect a metrics recorder,
  agent runtime, navigation worker, or world directly.
- **BOT-FG-007**: structured logs MUST use the existing bounded foreground tracing
  capture. Per-decision or per-sensorium records MUST NOT be encoded as routine
  log events: at 10-30 Hz across up to 32 agents they are a separate high-volume
  diagnostic signal. Logs are reserved for lifecycle, model/config activation,
  recovered degradation, queue saturation, invalid actions, budget exhaustion,
  and rate-limited fallback/status summaries.
- **BOT-FG-008**: decision, sensorium, and memory records MUST reach the UI through
  a dedicated bounded, lossy, non-blocking channel. The producer performs only
  `try_send`-equivalent work and counts a full queue instead of waiting. The UI
  owns its bounded per-agent history, exposes dropped/stale status, and MAY pause
  display without pausing or applying backpressure to agent execution.
- **BOT-FG-009**: Prometheus metrics MUST remain aggregate and low-cardinality.
  Agent, match, player, entity, decision sequence, tick, model hash, body region,
  memory token, and arbitrary reason text MUST NOT be metric labels. Per-agent
  sensorium, memory, and decision detail belong only to the bounded foreground
  record stream.
- **BOT-FG-010**: foreground mode is read-only in v1. It MUST NOT change goals,
  targets, difficulty, policy, model, perception, physiology, RNG state, or input.
  Diagnostic records are process-memory-only by default and MUST NOT be confused
  with the bot's semantic `AgentMemory`, consented human trace capture, or an
  authoritative replay journal.
- **BOT-FG-011**: the diagnostic stream MUST remain process-local in v1.
  Foreground mode MUST NOT add a remotely reachable per-agent diagnostics API;
  future remote inspection requires a separately authenticated, rate-limited,
  redacted design.

### Sensorium page visual contract

The Sensorium page shares the selected agent with Agents and Decisions and is a
live explanation surface, not a generic entity inspector. A representative
wide-terminal composition is:

```text
┌ Sensorium — agent 07 ─ observation 18420 ─ fresh 18 ms ─ policy BC-v3 ┐
│ VISION                         │ HEARING                               │
│ semantic FOV / salient cues    │ polar bearings + uncertainty wedges  │
│ visibility + impairment state │ energy bands + masking + event age   │
├────────────────────────────────┼───────────────────────────────────────┤
│ BODY / THERMAL                 │ CAPACITY / STATUS                     │
│ body-region condition map      │ stamina, breath, pain, suppression   │
│ wounds, bleeding, temperature │ equipment, ammo, reload, action gates│
├────────────────────────────────┴───────────────────────────────────────┤
│ MEMORY / BELIEF                                                       │
│ spatial: ● seen  ◌ remembered  ⋯ inferred  (uncertainty grows/fades) │
│ timeline: heard shot -2.1s → path blocked -1.4s → memory expired     │
├───────────────────────────────────────────────────────────────────────┤
│ EFFECTIVE PERFORMANCE                                                 │
│ reaction 182 ms = base 145 + fatigue 22 + pain 15                    │
│ aim 0.63 = base 0.91 - arm injury 0.16 - suppression 0.12            │
│ move 3.8 m/s = base 5.2 - leg injury 0.9 - encumbrance 0.5           │
├───────────────────────────────────────────────────────────────────────┤
│ DECISION LINK: admitted fields | still reaction-gated | overrides    │
└───────────────────────────────────────────────────────────────────────┘
```

The values above illustrate layout only; gameplay owns units, scales, and
modifier semantics.

- **BOT-FG-012**: agent selection MUST persist across Agents, Sensorium, and
  Decisions. The header MUST show observation tick, record age/freshness,
  capability availability, policy/model version, and whether the view is live,
  paused, or browsing bounded history.
- **BOT-FG-013**: Vision MUST use a semantic frustum/viewport diagram showing
  only stimuli actually admitted by the bot sensory model. Direction, distance
  band, salience, occlusion, confidence, memory age, and visibility impairment
  MUST be visually distinguishable. Hidden or merely replicated entities MUST
  NOT appear even to the diagnostic operator on this page.
- **BOT-FG-014**: Hearing MUST use a listener-centred polar display. A sound is a
  bearing wedge whose width reflects directional uncertainty; visual weight
  reflects received energy and age. Frequency-band energy, masking, class, and
  observation age MAY be shown, but no exact source position or identity may be
  reconstructed.
- **BOT-FG-015**: Body/Thermal MUST use a body-region silhouette or compact region
  grid plus thermal scale. It MUST distinguish injury, wound, bleeding, pain,
  mobility, and temperature/exposure when those capabilities exist. Unavailable
  channels render as `—`/`not provided`, never as healthy green.
- **BOT-FG-016**: Capacity/Status MUST use bars, bands, and explicit gates for
  stamina, breath/oxygen debt, fatigue, pain, suppression/stress, consciousness,
  encumbrance, equipment/ammunition, reload/jam, cooldowns, and status effects.
  Only capabilities present in `PlayerSensorium` are rendered.
- **BOT-FG-017**: Effective Performance MUST render the gameplay-supplied
  `PerformanceEnvelope` as a base-to-effective waterfall or ordered modifier
  list. A value that affected aim, reaction, movement, sensing, cadence, or action
  availability MUST link to its originating sensorium field and stable reason.
- **BOT-FG-018**: the page MUST distinguish three concepts: availability to the
  agent, inclusion in this policy decision, and causal application by a
  deterministic constraint. For an opaque NN, inclusion in its input MUST NOT be
  presented as proof that the model relied on that feature.
- **BOT-FG-019**: because v1 policy perception is semantic rather than pixel-based,
  a renderer camera image MUST NOT be labelled "what the bot saw". A contextual
  human-camera thumbnail MAY be displayed only with that limitation visible. If
  a future policy consumes pixels, diagnostics MUST show the exact cropped,
  resized, normalized policy input (or a faithful view of it), not a separately
  rendered diagnostic camera.
- **BOT-FG-020**: pausing or browsing Sensorium history freezes only UI selection.
  The agent continues normally, incoming records remain bounded/latest-wins, and
  overflow is counted. Returning to live mode MUST jump to the newest complete
  sensorium/memory/decision set without replaying intermediate state into the
  agent.
- **BOT-FG-021**: Memory MUST include an egocentric spatial view over only legal
  static prior knowledge and the bot's remembered dynamic evidence. Current
  sensed items use a solid mark, remembered last-known regions fade with
  confidence, inferred regions use a distinct dashed/dotted mark, and uncertainty
  MUST visibly expand. The UI MUST NOT move an unsensed marker using current world
  state.
- **BOT-FG-022**: Memory MUST include a bounded chronological timeline of
  observations, corroborations, contradictions, reacquisitions, expiries,
  forgotten items, plan failures, and important action outcomes. Time is relative
  to the selected observation/decision so the operator can follow cause and
  effect without wall-clock ambiguity.
- **BOT-FG-023**: selecting a spatial marker or timeline entry MUST show its local
  memory token, modality/kind, first and last observation age, confidence,
  uncertainty, decay/expiry, status, and linked observation/decision sequences.
  It MUST NOT expose an authoritative entity ID or hidden current transform.
- **BOT-FG-024**: the current decision MUST highlight exactly which memory items
  were included in its policy observation, which explicit belief/intent consumed
  them, and whether each item was observed, remembered, or inferred. Available
  but unused memories MUST remain visually distinct.
- **BOT-FG-025**: observed-but-reaction-gated memory MUST be visible beside the
  older value still admitted to the policy. Diagnostics MUST make this temporal
  split explicit rather than making the bot appear to ignore a cue it is not yet
  allowed to react to.
- **BOT-FG-026**: opaque recurrent-policy state MUST be rendered only as technical
  health metadata (version, shape, age, reset reason, finite/non-finite status,
  and used/not-used). The UI MUST NOT generate semantic labels for hidden tensor
  dimensions.
- **BOT-FG-027**: cooked map/navigation knowledge and fixed learned policy priors
  MUST use a separate visual layer from match-acquired memories. A legend MUST
  prevent authored prior knowledge from appearing as something the bot saw or
  learned during the current episode.
- **BOT-FG-028**: on narrow terminals, Sensorium MUST expose stable Senses, Body,
  Memory, and Performance subviews rather than dropping memory or performance
  panels. The same agent, observation, and decision selection MUST persist while
  moving between subviews.

### Agent metric families

The v1 foreground overview and Prometheus endpoint MUST be backed by these
low-cardinality families; exact buckets require representative load tests:

| Metric                                                        | Meaning                                                                            |
| ------------------------------------------------------------- | ---------------------------------------------------------------------------------- |
| `blackflower_agent_active_agents`                             | Current number of hosted active agents                                             |
| `blackflower_agent_decisions_total{source,outcome}`           | Completed, held, rejected, and fallback decisions by bounded policy source/outcome |
| `blackflower_agent_decision_duration_seconds{source}`         | End-to-end decision cost outside the deterministic tick                            |
| `blackflower_agent_inference_duration_seconds{outcome}`       | Local NN inference latency and result                                              |
| `blackflower_agent_perceived_entities`                        | Distribution of sensed-entity counts per decision                                  |
| `blackflower_agent_navigation_query_duration_seconds{result}` | Bounded navigation-worker query cost and result                                    |
| `blackflower_agent_fallbacks_total{reason}`                   | Neutral or classical fallbacks by bounded reason                                   |
| `blackflower_agent_decision_budget_exhaustions_total`         | Decisions skipped or curtailed by the shared budget                                |
| `blackflower_agent_memory_items{kind,status}`                 | Aggregate current semantic-memory occupancy by bounded kind/status                 |
| `blackflower_agent_memory_evictions_total{reason}`            | Semantic memories removed by bounded expiry/capacity policy                        |
| `blackflower_agent_diagnostic_records_dropped_total{kind}`    | Sensorium, memory, or decision records discarded by a full bounded queue           |

## Observability, testing, and fairness

- **BOT-TEST-001**: agents MUST be runnable headless within the existing network
  gate harness (smoke/nominal/degraded), including under packet loss and
  latency, without special-casing agents versus humans.
- **BOT-TEST-002**: the project MUST define a human-likeness acceptance measure
  (e.g. blind human evaluators cannot classify agent vs human above an agreed
  threshold, and/or agent input-distribution statistics match a human baseline
  within tolerance) and MUST gate v1 on it.
- **BOT-TEST-003**: agents MUST emit metrics for decision-tick cost, inference
  latency, perception set size, path-query cost, fallbacks, budget pressure, and
  dropped diagnostics using the BOT-FG metric contract.
- **BOT-TEST-004**: tests MUST prove that a `DecisionRecord` is derived from the
  same sensorium/intermediate values and final submission used by the live
  decision, that held plans retain their originating decision sequence, and that
  every action replacement or neutral fallback is visible.
- **BOT-TEST-005**: identical seeded scenarios MUST emit identical controls with
  diagnostics disabled, enabled, filtered, paused, and saturated. Queue
  saturation MUST be bounded and MUST NOT delay the decision or simulation path.
- **BOT-TEST-006**: foreground tests MUST cover narrow-terminal fallback,
  keyboard navigation, stale metrics, log/decision/sensorium drops, agent churn,
  NN and classical records, missing capabilities, and at least 32 concurrently
  visible agent summaries.
- **BOT-TEST-007**: every sensorium provider MUST have a parity test proving that
  its values, precision, freshness, and uncertainty are no stronger than the
  corresponding human feedback contract. Tests MUST cover `unavailable` versus
  zero, stale versus reaction-gated, coarse versus exact state, and redacted
  acoustic/impact origins.
- **BOT-TEST-008**: performance-envelope tests MUST apply multiple simultaneous
  modifiers and prove that the controller output and displayed ordered
  contributors use the same gameplay-owned derivation. The UI MUST NOT silently
  omit a contributor that changed the effective result.
- **BOT-TEST-009**: memory tests MUST prove confidence decay, uncertainty growth,
  bounded inference, expiry/eviction, corroboration, contradiction, reacquisition,
  and every episode/rebind reset. Moving an unsensed authoritative entity MUST NOT
  move its remembered marker or otherwise change agent output.
- **BOT-TEST-010**: foreground tests MUST prove the same memory item and status are
  used by policy input, decision evidence, spatial view, and timeline. They MUST
  distinguish current, remembered, inferred, reaction-gated, expired, and static
  prior layers, including narrow-terminal subviews.
- **BOT-SEC-001**: a fairness audit MUST confirm no code path lets an agent read
  state outside its interest set or act below human input latency. Any violation
  is a release blocker.
- **BOT-SEC-002**: the same agent binary/config MUST NOT be usable as a client
  cheat aid; agent perception MUST derive solely from the sanctioned harness
  view so it grants no capability a legitimate client lacks.
- **BOT-SEC-003**: sensorium/memory/decision records and foreground logs MUST be
  covered by the observability redaction policy. A fairness test MUST reject any
  field that cannot be derived from the legal `ClientView`, client-visible
  acoustic or feedback events, cooked client-visible data, agent-local memory,
  policy output, or downstream controller state.

## Delivery sequence (non-normative)

The current tracked codebase provides the shared `ClientHarness`/`ClientView`
input and prediction boundary, static Detour queries, the `rtcOccluded1`
visibility predicate, privacy-preserving `AcousticObservation`, the
`blackflower-agent` runtime composition, aggregate agent metric registry, a
bounded process-local diagnostic channel, and Agents/Sensorium/Decisions
foreground surfaces. The detailed pages remain empty unless a real controller
publishes exact immutable records; the current executable shell does not create
sample sensorium or decisions. Gameplay-owned `PlayerSensorium`, semantic
`AgentMemory`, perception/policy/controller implementations, and their parity
tests remain outstanding. The remaining v1 work should preserve this order:

1. Define the gameplay-owned, versioned `PlayerSensorium`, capability catalog,
   `PerformanceEnvelope`, semantic memory item/status schemas, precision,
   freshness/decay rules, and human-feedback parity tests before choosing NN
   feature-vector details.
2. Create `blackflower-agent` with dependency-boundary checks and gameplay-owned
   `BotObservation`, semantic `BotAction`, stable reason enums, bounded
   `AgentMemory`, `SensoriumSnapshot`/`MemorySnapshot`/`DecisionRecord`, and
   no-op-by-default observer interfaces.
3. Implement the gameplay snapshot/event adapters, visual and acoustic sensory
   filters, bounded perceptual/spatial/episodic/working memory, decaying belief,
   reaction gates, classical intent/decision model, human-likeness/performance
   constraints, navigation-worker DTO boundary, local steering, and neutral
   fallback.
4. Build the bounded 10-30 Hz decision scheduler and 60 Hz motor/input controller
   for both real-transport and in-process harness hosts, including aggregate CPU
   budgets and plan expiry.
5. Complete the existing agent metric families, structured lifecycle/anomaly
   logs, dedicated diagnostic stream, and Agents/Sensorium/Decisions/Overview
   surfaces with the real schemas and controller records delivered by steps
   1-4; never substitute UI-only sample state.
6. Add the harness `TraceObserver`, explicit trace framing, consented capture, the
   offline shared sensorium/perception encoder, dataset validation, behavioral-
   cloning training, signed model packaging, and local ONNX inference with the
   classical fallback intact.
7. Run network-gate, overload, fairness, sensorium/memory parity, redaction,
   determinism-isolation, human-likeness, foreground, and 32-participant
   acceptance suites before v1 is considered complete.

## Out of scope for v1

- Reinforcement-learning "challenge" opponents beyond the labeled tier of
  BOT-NN-001.
- Natural-language squad chatter/callouts (a separate, non-real-time concern).
- Cross-match online learning; v1 models are trained offline and shipped fixed.
- A remote sensorium/memory/decision inspection service or permanent foreground
  record archive.
