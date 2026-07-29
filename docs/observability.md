# Observability policy

Blackflower treats metrics, traces, logs, profiles, and deterministic replay as
different diagnostic signals:

- metrics detect aggregate health and drive alerts;
- traces correlate work and locate latency;
- logs describe discrete operational events;
- profiles identify CPU and memory hotspots;
- replay and versioned diagnostic bundles prove authoritative state evolution.

Logs and traces are not an authoritative journal.

## Ownership

Runtime libraries emit signals but do not install process-global backends:

- `blackflower-ecs` reports Flecs lifecycle, callback, allocation, and tick data;
- `blackflower-simulation` reports authoritative fixed-step execution;
- `blackflower-prediction` reports forward prediction and reconciliation;
- `blackflower-presentation` reports variable-step presentation frames;
- `blackflower-observability` installs the process tracing subscriber,
  non-blocking log writer, Prometheus recorder/exporter, and selected profiler
  backend.

Executables must initialize observability before constructing runtime worlds.
The authoritative server exposes Prometheus metrics on `127.0.0.1:9000` by
default. Client and harness defaults do not open a metrics listener. Failure to
start the exporter is logged but does not prevent the process from continuing.

## Deterministic boundary

Instrumentation must never:

- consume authoritative random numbers;
- inspect wall-clock time to make a simulation decision;
- change system registration, ordering, inputs, or state hashes;
- block a simulation tick while exporting data;
- make simulation success depend on an exporter or collector.

Durations use a monotonic clock outside simulation decisions. Operational
timestamps use UTC at the collector. Simulation causality uses the match,
authoritative tick, input sequence, protocol/content identity, and sealed state
hash.

Authoritative outputs refer only to state validated and sealed at `SealTick`.
The server scheduler owns pacing, tick-lag, catch-up, and deadline-pressure
metrics because wall-clock pacing is outside `SimulationWorld`.

## Logging

Compact colored text is the default for every executable. `LogFormat::Pretty`
selects a colored multi-line format for interactive diagnosis.
`LogFormat::Json` remains available for production ingestion and never contains
ANSI color sequences. `RUST_LOG` controls filtering, with `info` as the
fallback.

Levels have stable meanings:

- `ERROR`: invariant violation, failed callback, required state loss, match
  abort, corruption, or security failure;
- `WARN`: recovered degradation, bounded hard resync, saturated queue, or
  rate-limited anomaly;
- `INFO`: process and match lifecycle, effective non-secret configuration, and
  content/protocol activation;
- `DEBUG`: protocol decisions, reconciliation summaries, and bounded payload
  metadata;
- `TRACE`: hot-path ticks, phases, systems, and packet detail.

Routine ticks, packets, snapshots, entity transforms, and player positions are
not log records. Repeated warnings must be aggregated or rate-limited. The
non-blocking writer has a bounded lossy queue; dropped records are reported
through `blackflower_observability_log_lines_dropped_total` whenever
`ObservabilityGuard::report_health` is polled and at orderly shutdown.

Messages use the fewest lowercase words that preserve meaning. Targets,
event names, fields, and errors carry context instead of repeating it in the
message; for example, use `callback failed` rather than
`Rust system callback failed`.

Never log credentials, session tokens, raw voice/chat, complete untrusted
payloads, or full IP addresses. Player identifiers must be pseudonymous.

## Tracing

Trace parents are created at the layer that owns the correlation identity.
Server scheduling should parent the generic ECS span with `match_id`, `tick`,
and the relevant sealed state hash. Prediction spans include the local tick and
whether the pass is forward execution or re-simulation. Presentation spans
include the frame index and validated delta.

Operation spans for simulation, ECS execution, prediction, reconciliation, and
presentation are emitted at `INFO` so they remain available to Tracy in release
builds. Routine events inside those operations remain at `TRACE` and are
disabled in normal release builds by the workspace `release_max_level_info`
setting. Production diagnosis should retain full traces for errors and slow
operations and sample successful work. A future bounded per-match flight
recorder may preserve compact tick summaries for retroactive anomaly bundles.

## Metrics

Metric names use snake case, a subsystem prefix, and a unit suffix where
applicable. Counters end in `_total`. Allowed labels are bounded enums such as
`phase`, `pass`, `result`, `reason`, `direction`, and `transport`.

Never use match, connection, player, entity, tick, IP, content hash, arbitrary
error text, or other unbounded data as a metric label.

The initial domain metrics are:

| Metric | Meaning |
| --- | --- |
| `blackflower_simulation_ticks_total{result}` | Authoritative tick outcomes |
| `blackflower_simulation_system_executions_total{phase}` | Authoritative system executions aggregated by phase |
| `blackflower_simulation_tick_duration_seconds` | Authoritative tick compute time |
| `blackflower_simulation_deadline_misses_total` | Tick compute time above the fixed-step budget |
| `blackflower_prediction_ticks_total{pass,result}` | Forward and re-simulated prediction outcomes |
| `blackflower_prediction_tick_duration_seconds{pass}` | Prediction tick compute time |
| `blackflower_prediction_reconciliations_total{result,reason}` | Reconciliation decisions |
| `blackflower_prediction_reconciliation_duration_seconds` | Reconciliation wall time |
| `blackflower_prediction_resimulated_ticks` | Work repeated by one reconciliation |
| `blackflower_presentation_frames_total{result}` | Presentation frame outcomes |
| `blackflower_presentation_frame_duration_seconds` | Presentation compute time |
| `blackflower_presentation_frame_delta_seconds` | Validated variable frame delta |

At 240 Hz, an authoritative tick has a theoretical wall-clock budget of about
4.17 ms. Alert policy should prioritize sustained deadline misses, tick lag,
invariant or replay divergence, missing sealed snapshots, queue saturation, and
unbounded memory growth. Final thresholds require representative harness
measurements.

## Profiling

Operation spans are emitted once through `tracing` and exported by the
`tracing-tracy` layer. The backend-neutral `profiling` crate remains responsible
for frame marks and Flecs system callback scopes; system names are profiler
tags, not metric labels. The Tracy layer receives spans and explicit frame
marks, but not ordinary log events. It starts on demand, does not broadcast
discovery packets, and only accepts viewer connections from the same host:

```sh
RUST_LOG=info cargo run --release -p blackflower-server --features profile-with-tracy
```

Connect the Tracy Profiler UI to `127.0.0.1` while the process is running. The
feature is opt-in because it compiles the native Tracy integration and profiler
scopes are high-volume. Capture profiles against a release-like build and
record the Git commit, content identity, seed, map, player/bot count, and tick
interval. Production capture must be authenticated, time-bounded, limited to
one concurrent capture per instance, and disabled by default.

## Validation

Every observability change must preserve:

- identical authoritative results with and without an active recorder/subscriber;
- operation when collectors are unavailable;
- bounded labels and stable metric/event names;
- correct redaction of credentials and personal data;
- measured overhead under representative multi-world load.

The harness integration test executes simulation, prediction, and presentation
with and without active metrics/tracing collectors and compares the resulting
world progress.
