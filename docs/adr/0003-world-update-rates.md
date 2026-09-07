# Run simulation and prediction at 240 Hz with independent presentation

Status: accepted, explicitly requested by the owner; implementation and
performance evidence are pending.

The server Simulation World and client Prediction World advance at 240 Hz using
a fixed step of 1/240 seconds. The Presentation World targets 60 Hz and receives
measured variable elapsed time between presentation updates. This replaces the
earlier tentative 60 Hz simulation step and keeps rendering delays from changing
simulation time increments.

Normal operation yields about four prediction ticks per presented frame, but
scheduling must preserve independent clocks rather than hard-code four physics
iterations into each frame. Replay uses the same fixed step. Network
transmission and agent decision frequencies are separate concerns, not
implicitly 240 Hz requirements.

The higher simulation frequency reduces the time between normal simulation
updates while giving each tick approximately 4.167 ms of wall-clock budget. GPU
completion, reconciliation, and four simultaneous clients sharing the rendering
GPU must fit the existing measured MVP targets. The
[phase proposal](../flecs-phases.md) records the execution model; catch-up
limits and overload recovery remain open. Selecting these rates establishes a
requirement, not a hard real-time or performance guarantee.
