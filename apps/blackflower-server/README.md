# Blackflower authoritative server

`blackflower-server` is the dedicated authoritative server application. It
owns process startup, observability, fixed-rate simulation scheduling, orderly
shutdown, and an optional foreground diagnostics dashboard.

## Current responsibilities

- Run `SimulationWorld` on a dedicated owning thread at 240 Hz.
- Keep wall-clock pacing outside the simulation world.
- Initialize structured logging, Prometheus metrics, host metrics, and health
  reporting.
- Expose Prometheus metrics on `127.0.0.1:9000/metrics` by default.
- Shut down the simulation thread cleanly after `Ctrl-C` or foreground UI exit.
- Provide reusable server-side QUIC, admission, replication, scheduling, input,
  resynchronization, and voice-session composition in the library crate.

## Executable boundary

The current executable starts the authoritative simulation host and diagnostics
surface. It does not yet construct a QUIC listener, supply gameplay command
schemas, or connect admitted peers to concrete gameplay systems.

Those deployment- and gameplay-specific inputs remain outside the application
entry point. The `blackflower_server` library exposes `DedicatedServerNetwork`
and `NetworkPeer` for that composition while preserving bounded ingress,
connection admission, compatibility checks, applied-snapshot acknowledgement,
and replication scheduling.

## Run

Start the server from the repository root:

```bash
RUST_LOG=info cargo run --package blackflower-server --locked
```

The process runs until `Ctrl-C`, then joins the simulation thread and reports
the number of completed ticks.

### Foreground dashboard

Run the interactive metrics and logs dashboard in a terminal:

```bash
cargo run --package blackflower-server --locked -- --foreground
```

Set the initial capture/view level and structured-log regex when needed:

```bash
cargo run --package blackflower-server --locked -- \
  --foreground \
  --log-level debug \
  --log-regex 'network|deadline'
```

`--log-level` accepts `off`, `error`, `warn`, `info`, `debug`, or `trace`.
Foreground mode requires interactive standard input and output.

The dashboard exposes Overview, Logs, Simulation, Network, World, and Host
pages. Its primary controls are:

| Key | Action |
| --- | --- |
| `Tab` / `Shift+Tab` | Select the next or previous page |
| `1`-`6` | Open a page directly |
| `?` | Show help |
| `q` / `Ctrl-C` | Exit |
| `l` / `L` | Change the log view or capture level |
| `/` / `Escape` | Edit or clear the log regex |
| `p` / `End` | Pause or resume log following |
| Arrow keys / `PageUp` / `PageDown` | Navigate logs |
| `c` | Clear displayed logs |

## Profiling

Build and run the release server with Tracy profiling support:

```bash
RUST_LOG=info cargo run --release \
  --package blackflower-server \
  --features profile-with-tracy
```

## Validation

Run the server test suite from the repository root:

```bash
cargo test --package blackflower-server --locked
```

## Related documentation

- [Workspace overview](../../README.md)
- [Networking v1](../../docs/networking-v1.md)
- [Authoritative simulation world](../../crates/blackflower-world-simulation/README.md)
- [Observability and foreground dashboard](../../docs/observability.md)
