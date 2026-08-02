# Blackflower network protocol v1

Status: normative implementation contract for protocol revision 1.

The keywords **MUST**, **MUST NOT**, **SHOULD**, and **MAY** are normative.
Every integer on the application wire is explicitly little-endian unless a
QUIC primitive defines its own encoding. QUIC varints retain RFC 9000 encoding.

## Scope and ownership

- **NET-ENV-001**: v1 MUST operate on a hostile public Internet for one
  dedicated regional server and at most 32 players per match.
- **NET-ENV-002**: the design target is regional RTT p95 at or below 60 ms.
- **NET-ENV-003**: networking MUST NOT define fictitious gameplay schemas,
  weapons, projectiles, bots, human input, rendering, or matchmaking.
- **NET-ENV-004**: application protocol, replication, and transport concerns
  MUST remain separate. QUIC owns cryptography, transport ACKs, congestion
  control, pacing, loss recovery, and path validation.
- **NET-ENV-005**: `SnapshotAppliedAck` is an application fact and MUST NOT be
  inferred from a QUIC transport ACK.

## QUIC and TLS

- **NET-QUIC-001**: every active game session MUST use one QUIC connection over
  UDP with TLS 1.3 and ALPN `blackflower/1`.
- **NET-QUIC-002**: 0-RTT MUST be disabled. Code MUST NOT call `into_0rtt`.
- **NET-QUIC-003**: the server MUST send stateless Retry before accepting an
  incoming address that Quinn has not validated.
- **NET-QUIC-004**: application-level active connection migration MUST be
  disabled. Quinn path validation remains enabled because Quinn 0.11 otherwise
  drops NAT port rebinding together with active migration. The adapter MUST
  close a validated IP-changing path, MAY accept a validated port-only NAT
  rebind, and MUST emit `PathChanged` so the eight-sample time synchronization
  burst restarts.
- **NET-QUIC-005**: the peer MUST advertise a QUIC DATAGRAM capacity of at least
  1011 bytes: 1000 application bytes plus the 11-byte common header. A
  connection without DATAGRAM or below that capacity MUST be rejected. There is
  no stream fallback.
- **NET-QUIC-006**: the client MUST initiate exactly one bidirectional
  `SessionControl` stream. The server MUST initiate at most one concurrent
  unidirectional `StateBootstrap` stream. All other application flows use
  DATAGRAM.
- **NET-QUIC-007**: the transport adapter MUST expose only `QuicServer`,
  `QuicClient`, `ServerNetworkHandle`, `ClientNetworkHandle`, and supporting
  endpoint/TLS configuration. `QuicClient` is not a game client.
- **NET-TLS-001**: clients MUST trust only the configured current service CA
  root and optional next root during rotation. The server MUST NOT request a
  client certificate.
- **NET-TLS-002**: service leaf certificates SHOULD have a lifetime no longer
  than 24 hours. CA operation and leaf issuance remain deployment concerns.
- **NET-ADMIT-001**: server construction MUST require finite per-origin attempt
  and pending-handshake limits; an unlimited default is forbidden.

## Wire contract

- **NET-WIRE-001**: application codecs MUST be explicit and MUST NOT use
  `rkyv`, `serde`, `bincode`, Rust layouts, host byte order, or unsafe casts.
- **NET-WIRE-002**: every DATAGRAM MUST begin with exactly 11 bytes:

  | Offset | Bytes | Field |
  | ---: | ---: | --- |
  | 0 | 1 | `wire_version` |
  | 1 | 1 | `flow_id` |
  | 2 | 4 | `connection_epoch` |
  | 6 | 4 | `flow_sequence` |
  | 10 | 1 | reserved flags, zero in v1 |

- **NET-WIRE-003**: v1 flow IDs are `TimeSync`, `Input`, `SnapshotDelta`,
  `SnapshotAppliedAck`, `VoiceCapture`, and `VoiceDelivery`.
- **NET-WIRE-004**: reliable streams MUST begin with the four-byte application
  preamble `wire_version`, `stream_kind`, and two zero reserved bytes.
- **NET-WIRE-005**: reliable messages MUST use an application kind followed by
  a canonical QUIC varint length and the exact payload.
- **NET-WIRE-006**: one control payload MUST NOT exceed 16 KiB. One uncompressed
  bootstrap body MUST NOT exceed 2 MiB.
- **NET-WIRE-007**: decoders MUST reject unsupported versions, unknown kinds,
  unknown flows, non-zero reserved bits, invalid counts, truncation, trailing
  bytes, and oversized lengths before allocating the declared value.
- **NET-WIRE-008**: `ProjectionDigest` MUST be BLAKE3-256 over the domain
  `blackflower.snapshot.projection.v1\0`, protocol revision, snapshot tick, and
  reconstructed canonical client projection.

## Admission, lifecycle, and reconnect

- **NET-SESSION-001**: the lifecycle is `Connecting`, `Secure`,
  `Authenticating`, `Compatible`, `Synchronizing`, `Active`,
  `Resynchronizing`, and `Closing`. All transitions MUST be explicit.
- **NET-SESSION-002**: an opaque `AdmissionTicket` MUST be at most 4 KiB,
  expire within 60 seconds, and be consumed atomically once by
  `SessionAuthority`.
- **NET-SESSION-003**: admission MUST compare `ProtocolRevision`,
  `SimulationCompatibilityId`, and `RequiredContentSetId` for exact equality
  before authoritative player state is created.
- **NET-SESSION-004**: activation MUST be aligned to four ticks and scheduled at
  least 24 ticks plus current uncertainty into the future. A peer remains
  `Synchronizing` until that tick is reached.
- **NET-SESSION-005**: post-activation full resync is limited to two attempts in
  a rolling 60-second window.
- **NET-RESUME-001**: reconnect is allowed for 30 seconds with an opaque one-use
  resume token. Consumption MUST issue a fresh increasing `connection_epoch`
  and identify the old session connection for invalidation.
- **NET-RESUME-002**: reconnect MUST perform a full snapshot, time sync, and new
  activation. It MUST NOT resume from an incremental baseline alone.

## Clock synchronization

- **NET-CLOCK-001**: admission MUST take eight four-timestamp samples at 100 ms
  intervals. `Active` MUST take one sample per second. A validated path change
  MUST restart the initial burst.
- **NET-CLOCK-002**: the filter MUST select the minimum-delay retained sample,
  estimate uncertainty as half its network delay, and slew mapped time without
  moving the mapped clock backwards.
- **NET-CLOCK-003**: first activation requires uncertainty at or below two
  simulation ticks. Uncertainty above four ticks for three consecutive samples
  yields `ClockDegraded`.
- **NET-CLOCK-004**: temporal commands MUST be blocked above eight ticks of
  uncertainty or after three seconds without a valid sample.
- **NET-CLOCK-005**: input lead is four-tick aligned from
  `srtt / 2 + 2 * rttvar`, clamped to 4 through 24 ticks. The initial lead is 12
  ticks.

## Input and command ingress

- **NET-INPUT-001**: one input DATAGRAM MUST carry the current control frame and
  up to two byte-identical predecessors. It MAY carry at most eight discrete
  commands. One control frame covers four simulation ticks.
- **NET-INPUT-002**: the same `InputSequence` or `CommandId` with different
  canonical content is a protocol violation. Exact duplicates are idempotent.
- **NET-INPUT-003**: a missing canonical input MAY be held for 12 ticks, then
  MUST become neutral and release held edges. A separate 240-tick input
  failsafe MUST be retained. Five seconds without authenticated application
  traffic closes the connection.
- **NET-CMD-001**: maximum lateness is eight ticks for movement and jump, 12 for
  interaction, 24 for reload/equip/use, 32 for `RewindRay`, and 16 for
  `CatchUpBallistic`. `CurrentTickOnly` accepts no lateness. Future commands
  beyond 24 ticks are rejected.
- **NET-CMD-002**: networking MUST emit `Queued`, `Committed`, `Rejected`, or
  `Superseded` dispositions, but MUST NOT execute gameplay.
- **NET-CMD-003**: `HistoricalCommandContext` is read-only. `RewindRay` may read
  32 ticks, `CatchUpBallistic` 16 ticks, and explosives/dynamic physics only the
  current tick.
- **NET-PRED-001**: prediction and input history MUST retain 512 ticks. One
  reconciliation MUST NOT roll back more than 64 ticks.

## Replication and interest

- **NET-REPL-001**: `ReplicatedEntityId` MUST be non-zero, monotonically
  allocated, and never reused within a session. Re-entry uses the same ID.
- **NET-REPL-002**: `ComponentId` MUST be a non-zero u16 in a stable registry
  owned by `ProtocolRevision`.
- **NET-REPL-003**: visibility MUST be projected into public, owner, team, and
  global component sets before serialization.
- **NET-REPL-004**: component values are full replacements with their own
  `ComponentSampleTick`. Components not sampled again MUST retain the earlier
  sample tick.
- **NET-REPL-005**: incremental deltas contain only `Spawn`, `Update`,
  `RemoveComponent`, and `Forget`. The API has no whole-entity compatibility
  shim.
- **NET-REPL-006**: scheduling priority is lifecycle, owner correction, active
  actor, then remaining state.
- **NET-AOI-001**: spatial entry radius is 512 m. Exit radius is
  `512 m + max(16 m, vmax * 0.5 s)`. Always-relevant state MUST be explicit.
- **NET-REPL-007**: the canonical quantizers are signed-centimetre position,
  signed-centimetre-per-second velocity, unsigned 16-bit turn angles, and a
  canonical signed 16-bit smallest-three quaternion.
- **NET-SNAPSHOT-001**: incremental snapshots target 30/s and MUST protect at
  least 15/s. One generation has at most four all-or-nothing chunks and a
  66.7 ms reassembly deadline.
- **NET-SNAPSHOT-002**: the sender retains at most 32 sent generations. A
  baseline is promoted only when both tick and `ProjectionDigest` exactly match
  `SnapshotAppliedAck`.
- **NET-SNAPSHOT-003**: full admission, hard resync, and reconnect snapshots use
  `StateBootstrap`; they are never represented as incremental chunks.

## Bootstrap, scheduling, and backpressure

- **NET-BOOT-001**: bootstrap is uncompressed, targets at most 512 KiB, has an
  absolute 2 MiB bound, a ten-second deadline, and a separately reserved
  2 Mbps transfer budget.
- **NET-QUEUE-001**: session control is limited to 64 messages and 1 MiB; one
  bootstrap may be active; input is latest-wins; snapshot history is 32
  generations; voice is three packets per stream and four streams; host events
  are limited to 128.
- **NET-BUDGET-001**: per-client upstream is 128/256 kbps, per-client downstream
  is 512/1024 kbps, and aggregate match egress is 16/32 Mbps. Aggregate limits
  win over per-client availability.
- **NET-BUDGET-002**: priority is session control and input, protected 15 Hz
  snapshots, voice inside its playout deadline, then additional snapshots up to
  30 Hz.
- **NET-BUDGET-003**: estimated application bytes MUST be reconciled with the
  cumulative UDP bytes reported by Quinn.

## Voice and operations

- **NET-VOICE-001**: existing `BFAD` v1 bytes remain unchanged. Voice is Opus
  mono in 20 ms packets, 24 kbps target, 40 kbps maximum, 60 ms jitter, and at
  most four audible deliveries. `blackflower-acoustics` owns that application
  codec; networking treats its payload as opaque bytes.
- **NET-VOICE-002**: every authenticated `VoiceStreamId` MUST be bound to exactly
  one of proximity, squad, or team routing. There is no application
  retransmission, fragmentation, or voice E2EE in v1.
- **NET-OPS-001**: `SnapshotStalled` begins after 500 ms without applied
  progress, `InputFailsafe` after one second, and connection unresponsive after
  five seconds without authenticated traffic.
- **NET-OBS-001**: metrics MUST cover connections, RTT, clock, queues, bytes,
  drops, snapshots, inputs, bootstrap, resync, voice, and violations. Session
  and player IDs MUST NOT appear in metric labels.

## Harness and client boundary

- **NET-HARNESS-001**: `blackflower-harness` MUST remain a library and the only
  shared binding from human or bot canonical input into networking and
  prediction. It MUST NOT contain device collection, presentation, or bot
  decision logic.
- **NET-HARNESS-002**: the harness MUST own the client application session,
  bootstrap and incremental projection application, applied-snapshot ACKs,
  input and command identities, bounded histories, prediction coordination,
  reconciliation, reconnect, and hard resync. It MUST reuse networking,
  replication, and `blackflower-world-prediction` rather than duplicate them.
- **NET-HARNESS-003**: human frontends and headless bots MUST consume the same
  immutable `ClientView` and submit the same canonical `ControlSubmission`.
  Neither may query authoritative ECS state through the harness.
- **NET-HARNESS-004**: `ClientView` MUST expose a bounded immutable window of
  fully reconstructed authoritative projections in tick order. Presentation
  MAY use it for interpolation but MUST NOT own or mutate replication history.

## Acceptance gates

- **NET-GATE-001**: the nominal gate runs 32 internal clients for 30 minutes at
  p99 RTT 100 ms, jitter 10 ms, and loss 1%.
- **NET-GATE-002**: the degraded gate runs 32 internal clients for 10 minutes at
  p99 RTT 180 ms, jitter 30 ms, and loss 5%.
- **NET-GATE-003**: the gate report MUST be deterministic JSON and record the
  profile, seed, duration, client count, thresholds, measured counters, and
  pass/fail result.
- **NET-GATE-004**: ordinary CI runs codec/state/replication tests, Clippy,
  formatting, deny, and a reduced smoke gate. Full nominal and degraded soaks
  run only on schedule or manual dispatch.

## Verification map

| Requirements | Primary verification |
| --- | --- |
| `NET-WIRE-*`, `NET-INPUT-*`, `NET-CLOCK-*`, `NET-SESSION-*` | `crates/blackflower-networking/tests/protocol.rs` |
| `NET-REPL-*`, `NET-AOI-*`, `NET-SNAPSHOT-*` | `crates/blackflower-networking-replication/tests/replication.rs` |
| `NET-QUIC-*`, `NET-TLS-*`, `NET-BOOT-*` | `crates/blackflower-networking-quic/tests/loopback.rs` |
| loss, jitter, reorder, duplication, outage, MTU, NAT rebinding | `crates/blackflower-networking-quic/tests/udp_proxy.rs` and `loopback.rs` |
| admission, full bootstrap, activation, reconnect composition | `apps/blackflower-server/tests/network.rs` |
| `NET-HARNESS-*` | `crates/blackflower-harness/tests/client.rs`, `scripts/check-test-layout.sh`, and workspace metadata checks |
| `NET-GATE-*` | `cargo xtask network-gate` smoke and scheduled/manual soak workflows |

The gate profiles run in wall-clock time: five seconds for `smoke`, thirty
minutes for `nominal`, and ten minutes for `degraded`. Each cadence step builds
and decodes the real revision-1 input wire bytes for 32 internal clients. The
full profiles run only from the scheduled or manually dispatched network-gate
workflow. Decoder, snapshot-reassembly, and session-transition fuzz entry
points live under `fuzz/fuzz_targets`.

## Conflict resolution

The approved choices have no unresolved semantic conflict after these
clarifications:

1. Quinn 0.11 cannot advertise `disable_active_migration` while accepting NAT
   rebinding: its implementation drops both. Therefore QUIC path validation is
   enabled internally and the adapter enforces the v1 policy after validation:
   port-only rebinding is accepted and IP-changing migration is closed;
2. the four-chunk limit applies only to incremental DATAGRAM snapshots; full
   admission and hard resync always use the bounded reliable bootstrap stream;
3. QUIC transport reliability is not duplicated, while application ACKs remain
   necessary because transport delivery does not prove projection application;
4. the low-level `QuicClient` is transport infrastructure and does not pre-empt
   the future harness or either future client composition.
