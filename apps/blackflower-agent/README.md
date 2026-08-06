# Blackflower headless agent

`blackflower-agent` is the process shell for one autonomous ordinary client. It
uses the same admission, snapshot, prediction, reconciliation, input, and QUIC
contracts as the player client. It has no access to authoritative ECS state.

## Current boundary

The reusable `AgentRuntime` composes only systems that already exist:

- `blackflower-networking-quic` for the authenticated client connection;
- `blackflower-harness` for session lifecycle, snapshots, controls, and
  prediction coordination;
- `blackflower-world-prediction` through the gameplay-supplied harness
  prediction implementation;
- `blackflower-navigation` for the cooked static Detour navmesh and query
  filter;
- `blackflower-observability` for logs, Prometheus metrics, host metrics, and
  optional Tracy profiling.

Every established `AgentRuntime` owns aggregate low-cardinality agent metrics.
`AgentRuntimeConfig::with_diagnostics` can additionally attach a process-local
controller identity. `AgentDiagnosticConfig::headless` retains aggregate
controller telemetry with no detail observer; foreground hosts attach a bounded
sender with `AgentDiagnosticConfig::new`. A controller publishes its
already-computed `SensoriumSnapshot` and `DecisionRecord` through
`AgentRuntime::diagnostics_mut`; publication uses `try_send`, counts a full queue,
and never waits on the terminal UI. The UI receives immutable records and cannot
inspect the harness, navigation runtime, policy, or mutable memory.
Controllers check `AgentDiagnostics::records_enabled` before allocating these
diagnostic-only projections, so headless operation with no observer pays no
snapshot construction cost.

The application deliberately does not define an observation encoder, policy or
model, target selection, steering, gameplay control schema, asset deployment
configuration, or background inference worker. `AgentRuntime::connect` accepts
validated transport/session/navigation inputs plus a gameplay prediction
implementation; a future controller must consume `ClientView` and submit
ordinary `ControlSubmission` values through the harness.

The diagnostic DTOs do not invent those missing systems. Their bounded fields
carry only exact summaries emitted by a real controller, distinguish unavailable,
stale, reaction-gated, and policy-admitted sensorium channels, and keep memory
tokens process-local. Without an attached runtime diagnostic stream, detailed
agent panels explicitly report that no runtime data is available.

Until deployment and gameplay inputs are provided, the executable runs as an
observable process shell and reports those systems as not configured.

## Run

Start the shell with metrics on `127.0.0.1:9001`:

```sh
RUST_LOG=info cargo run --package blackflower-agent --locked
```

The process waits for `SIGINT` (`Ctrl-C`) or, on Unix, `SIGTERM` and then flushes
observability normally. Override the loopback metrics address when several
agents share one host:

```sh
cargo run --package blackflower-agent --locked -- \
    --metrics-bind-address 127.0.0.1:9101
```

### Foreground dashboard

```sh
cargo run --package blackflower-agent --locked -- --foreground
```

The foreground exposes Overview, Logs, Agents, Sensorium, Decisions, Session,
Prediction, Navigation, and Host panels. Overview includes aggregate runtime
health, decision latency, and diagnostic drops. Agents, Sensorium, and Decisions
consume only the bounded real-runtime stream; the process shell never fills
them with sample activity. Missing runtime series render as unavailable.

| Key | Action |
| --- | --- |
| `Tab` / `Shift+Tab` | Select the next or previous panel |
| `1`-`9` | Open a panel directly |
| `?` | Show help |
| `q` / `Ctrl-C` | Exit |
| `Up` / `Down` | Select the same real agent across agent panels |
| `Left` / `Right` / `End` | Browse decision history or return to newest |
| `l` / `L` | Change the log view or capture level |
| `/` / `Escape` | Edit or clear the log regex |
| `p` / `End` | Pause or resume log following |
| Arrow keys / `PageUp` / `PageDown` | Navigate logs |
| `c` | Clear displayed logs |

## Validation

```sh
cargo test --package blackflower-agent --locked
```

The agent metric registry includes active/health gauges, decisions and
latencies, optional local inference latency, perceived-entity distribution,
navigation query latency, fallbacks, decision-budget exhaustion, semantic
memory occupancy/evictions, and diagnostic queue drops. Metric labels are all
bounded enums; agent IDs, policy versions, ticks, and arbitrary reasons stay out
of Prometheus.

## Related documentation

- [Workspace overview](../../README.md)
- [Shared client harness](../../crates/blackflower-harness/README.md)
- [Networking v1](../../docs/networking-v1.md)
- [Navigation runtime](../../crates/blackflower-navigation/README.md)
- [Observability policy](../../docs/observability.md)
