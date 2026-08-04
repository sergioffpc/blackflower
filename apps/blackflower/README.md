# Blackflower client

`blackflower` is the native desktop client shell. It owns the operating-system
window, device-input lifecycle, observability setup, and the client-side
presentation loop.

## Current responsibilities

- Create and manage the native window through `winit`.
- Capture keyboard, mouse-button, cursor, wheel, and raw mouse-motion input.
- Release cursor capture and neutralize input when focus or lifecycle state is
  lost, preventing stale input from leaking into later frames.
- Advance `PresentationWorld` at a target rate of 60 Hz while the window is
  active and visible.
- Optionally connect an established `ClientHarness` to presentation through
  `run_with_harness` and `PresentationBridge`.
- Initialize client observability and health reporting.

## Integration boundary

The executable currently calls `blackflower::run()`, which starts the native
presentation shell without constructing a network transport or prediction
runtime.

Gameplay clients can instead call `blackflower::run_with_harness(...)` with an
already configured `ClientHarness`. On every client update, the shared harness
is advanced first; its immutable client view and emitted events are then
captured by the presentation bridge before the presentation frame runs.

Transport construction, prediction policy, gameplay command encoding, and
renderer submission remain external integration boundaries. The window emits
redraw requests, but this application does not yet submit a render frame to a
renderer.

## Run

From the repository root:

```bash
RUST_LOG=info cargo run --package blackflower --locked
```

Left-click requests cursor capture; `Escape`, focus loss, suspension, and
application exit release it.

To enable Tracy profiling support:

```bash
cargo run --package blackflower --features profile-with-tracy --locked
```

## Validation

Run the client unit and integration tests from the repository root:

```bash
cargo test --package blackflower --locked
```

The tests cover input snapshot semantics, window lifecycle transitions, frame
clock behavior, and the harness-to-presentation handoff.

## Related documentation

- [Workspace overview](../../README.md)
- [Shared client and bot harness](../../crates/blackflower-harness/README.md)
- [Presentation world](../../crates/blackflower-world-presentation/README.md)
- [Observability](../../docs/observability.md)
