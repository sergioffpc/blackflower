# Blackflower client

`blackflower` is the native desktop network client. It owns the operating-system
window, device-input lifecycle, observability setup, and the client-side
presentation loop.

## Current responsibilities

- Create and manage the native window through `winit`.
- Capture keyboard, mouse-button, cursor, wheel, and raw mouse-motion input.
- Release cursor capture and neutralize input when focus or lifecycle state is
  lost, preventing stale input from leaking into later frames.
- Advance `PresentationWorld` at a target rate of 60 Hz while the window is
  active and visible.
- Establish authenticated QUIC, negotiate the compiled protocol revision, and
  verify the server-selected map against locally signed asset packages before
  running the shared `ClientHarness` beside presentation.
- Map captured WASD and relative mouse input into canonical schema-v1 controls
  at 60 Hz and submit them through the shared harness.
- Run client movement prediction at 240 Hz and reconcile authoritative state
  with explicit position, velocity, and orientation tolerances.
- Initialize client observability and health reporting.
- Optionally run a terminal dashboard beside the native window.

## Integration boundary

The executable always establishes QUIC, negotiates the protocol and content,
performs eight-sample clock synchronization, applies the authoritative movement
bootstrap, accepts the server-assigned control binding, and reaches the shared
session's `Active` state. The server address defaults to
`127.0.0.1:4433`; there is no offline or local-shell mode.

After protocol negotiation, the server sends `ContentManifest` with its selected
map and exact signed package-set identity. The client derives its local identity
from `--asset-package-directory`, sends `ContentReady` only for an exact match,
and rejects the session before bootstrap otherwise.

Gameplay clients can instead call `blackflower::run_with_harness(...)` with an
already configured `ClientHarness`. On every client update, the shared harness
is advanced first; its immutable client view and emitted events are then
captured by the presentation bridge before the presentation frame runs.

The built-in connected path validates schema-v1 authoritative movement
snapshots, maps captured WASD and mouse input to consecutive four-tick control
frames, advances `PredictionWorld`, and restores plus re-simulates when the
server state falls outside the protocol-v1 margins. Position may differ by up
to 2 cm, velocity by 5 cm/s, and orientation by 0.5 degrees; controlled entity
and grounded state still compare exactly. Renderer proxies and renderer
submission remain integration boundaries. The window emits redraw requests,
but this application does not yet submit a render frame to a renderer.

## Run

After creating the local TLS and signed-asset fixture described in the
[server README](../blackflower-server/README.md), run the client with:

```bash
cargo run --package blackflower --locked -- \
  --server-name localhost \
  --service-ca-certificate .local-network/service-ca.pem \
  --asset-package-directory target/assets/packages/debug \
  --asset-trust-key .local-network/asset-signing-public.pem
```

`--server-address` may override the default `127.0.0.1:4433`. Add
`--foreground` to show the same connection and presentation state in the client
terminal dashboard. Admission is credential-free until authentication and
matchmaking are composed. Service-CA rotation requires restarting the client
with the new CA certificate; overlapping roots are deliberately not accepted.

Left-click requests cursor capture; `Escape`, focus loss, suspension, and
application exit release it.

The dashboard exposes Overview, Logs, Session, Prediction, Runtime/World,
Presentation, and Host panels. Prediction reports the active prediction mode,
tick latency, and reconciliation outcomes; Runtime/World shows the live
presentation world's ECS state. It reads the process-local Prometheus endpoint at `127.0.0.1:9002`;
missing session or renderer signals remain visibly unconfigured. Closing the
native window or pressing `q`/`Ctrl-C` in the terminal stops both sides and
restores the terminal.

To enable Tracy profiling support:

```bash
cargo run --package blackflower --features profile-with-tracy --locked
```

## Validation

Run the client unit and integration tests from the repository root:

```bash
cargo test --package blackflower --locked
```

The tests cover input snapshot semantics, native movement-control mapping,
forward prediction and tolerance-based reconciliation, window lifecycle
transitions, frame-clock behavior, the harness-to-presentation handoff, and
every terminal panel.

## Related documentation

- [Workspace overview](../../README.md)
- [Shared client and bot harness](../../crates/blackflower-harness/README.md)
- [Presentation world](../../crates/blackflower-world-presentation/README.md)
- [Observability](../../docs/observability.md)
