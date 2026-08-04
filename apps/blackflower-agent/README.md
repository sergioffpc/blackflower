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

The application deliberately does not define an observation encoder, policy or
model, target selection, steering, gameplay control schema, asset deployment
configuration, or background inference worker. `AgentRuntime::connect` accepts
validated transport/session/navigation inputs plus a gameplay prediction
implementation; a future controller must consume `ClientView` and submit
ordinary `ControlSubmission` values through the harness.

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

The foreground exposes Overview, Logs, Session, Prediction, Navigation, and
Host panels. Missing runtime series render as unavailable instead of invented
activity. The navigation panel also states which future controller pieces are
deliberately absent.

| Key | Action |
| --- | --- |
| `Tab` / `Shift+Tab` | Select the next or previous panel |
| `1`-`6` | Open a panel directly |
| `?` | Show help |
| `q` / `Ctrl-C` | Exit |
| `l` / `L` | Change the log view or capture level |
| `/` / `Escape` | Edit or clear the log regex |
| `p` / `End` | Pause or resume log following |
| Arrow keys / `PageUp` / `PageDown` | Navigate logs |
| `c` | Clear displayed logs |

## Validation

```sh
cargo test --package blackflower-agent --locked
```

## Related documentation

- [Workspace overview](../../README.md)
- [Shared client harness](../../crates/blackflower-harness/README.md)
- [Networking v1](../../docs/networking-v1.md)
- [Navigation runtime](../../crates/blackflower-navigation/README.md)
- [Observability policy](../../docs/observability.md)
