# Blackflower observability TUI

`blackflower-observability-tui` contains reusable terminal-observability state
for Blackflower executables. It is intentionally separate from
`blackflower-observability`: process logging, metrics export, and profiling do
not depend on an interactive terminal UI.

The crate owns:

- bounded structured-log buffering, filtering, and navigation;
- bounded asynchronous polling of the process-local Prometheus endpoint;
- Prometheus sample parsing, exact-series counter rates, and histogram
  quantiles;
- shared terminal initialization and restoration around a dashboard loop.

Executables retain their own Ratatui page models and renderers because an
authoritative server, a player client, and a headless agent expose different
operational contracts. The current consumers are `blackflower-server`,
`blackflower`, and `blackflower-agent`.
