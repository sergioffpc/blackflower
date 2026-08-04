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
- Shut down the simulation thread cleanly after `SIGINT`, Unix `SIGTERM`, or
  foreground UI exit.
- Optionally bind the real QUIC supervisor with finite handshake/connection
  caps, exact protocol negotiation, server-owned map/content readiness,
  bootstrap, clock sync, and activation.
- Provide reusable replication, scheduling, input, resynchronization, and
  voice-session composition in the library crate.

## Executable boundary

The executable always starts the authoritative simulation host and diagnostics
surface. Supplying `--listen-address` plus TLS, map, and signed-package inputs
also starts the QUIC listener. The included `LoopbackSessionAuthority` assigns
credential-free process-local identities and is intentionally restricted to
loopback; authentication and matchmaking remain future composition boundaries.

The listener currently sends an empty bootstrap and does not supply gameplay
command schemas or project admitted peers into concrete ECS components. Those
gameplay inputs remain outside networking. The `blackflower_server` library
exposes `DedicatedServerNetwork` and `NetworkPeer` for that composition while
preserving bounded ingress, connection admission, compatibility checks,
applied-snapshot acknowledgement, and replication scheduling.

## Run

Start the server from the repository root:

```bash
RUST_LOG=info cargo run --package blackflower-server --locked
```

The process runs until `SIGINT` (`Ctrl-C`) or, on Unix, `SIGTERM`; it then joins
the simulation thread and reports the number of completed ticks.

### Local client/server vertical slice

Create a short-lived local CA, server certificate, asset signing key, and cooked
package from the repository root:

```bash
cargo xtask keys generate
cargo xtask assets cook \
  --profile debug \
  --package pak000 \
  --signing-key .local-network/asset-signing-key.pem
```

`keys generate` invokes OpenSSL and refuses to overwrite an existing output
directory. Use `--output <DIRECTORY>` or `--server-name <DNS_NAME>` to override
the `.local-network` and `localhost` defaults.

Start the real loopback listener:

```bash
cargo run --package blackflower-server --locked -- \
  --listen-address 127.0.0.1:4433 \
  --tls-certificate .local-network/server-chain.pem \
  --tls-private-key .local-network/server-key.pem \
  --map-id maps/bootstrap \
  --asset-package-directory target/assets/packages/debug \
  --asset-trust-key .local-network/asset-signing-public.pem
```

The server derives the exact content-set identity from those verified packages
and sends it with `maps/bootstrap`. Then start the client with the matching
command in the [client README](../blackflower/README.md). TLS and asset-signature
verification remain enabled; there is no skip-verification development path.

### Foreground dashboard

Run the interactive metrics and logs dashboard in a terminal:

```bash
cargo run --package blackflower-server --locked -- --foreground
```

Log capture and view levels start at `INFO`, with no regex, and are configured
inside the Logs panel. Foreground mode requires interactive standard input and
output.

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
