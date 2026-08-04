# blackflower-harness

`blackflower-harness` is the stateful client runtime shared by the player
frontend and headless bots. Both submit source-neutral canonical controls and
consume the same reconstructed snapshots, prediction state, and client-facing
events.

`ClientHarness` owns the application session above QUIC, protocol negotiation,
server-selected map/content readiness, resume transitions, full bootstrap and
incremental snapshot reconstruction, applied snapshot acknowledgements,
control/command identities, input redundancy, prediction coordination,
reconciliation, and hard-resync requests. It never collects devices, renders
presentation, or makes bot decisions.

`PredictionSession` reuses `blackflower-world-prediction` histories and
reconciliation. Gameplay supplies a `PredictionCodec` for canonical component
and input bytes plus a `ForwardPredictionDriver` that advances the existing
prediction pipeline. `ClientView` exposes immutable authoritative and predicted
state to either presentation or bot policy. It also exposes the bounded
chronological authoritative projection window retained by the snapshot inbox,
so presentation can interpolate without owning or duplicating network history.

The crate is a library. Low-level Quinn tasks remain in
`blackflower-networking-quic`, and the authoritative simulation sees only the
ordinary input datagrams produced here; it has no human/bot execution branch.
