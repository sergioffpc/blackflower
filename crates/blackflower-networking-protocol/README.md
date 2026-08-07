# Blackflower networking protocol

`blackflower-networking-protocol` is the revision-specific application schema
shared by the native client, authoritative server, and ordinary headless agents.
It composes the transport-independent envelopes from `blackflower-networking`
with the projection machinery from `blackflower-networking-replication`.

Revision 1 currently defines only movement and orientation:

- stable component IDs for transform, velocity, grounded character state, and
  owner-only prediction acknowledgement;
- canonical component encoders and strict decoders;
- an eight-byte movement control containing two normalized movement axes,
  absolute view yaw, and absolute view pitch;
- no discrete gameplay commands;
- explicit prediction tolerances for continuous state.

Its public API lives under `blackflower_networking_protocol::v1`. Names inside
that module are unversioned (`Transform`, `MovementControl`, and so on); the
module path identifies the wire revision.

The crate owns no sockets, QUIC tasks, ECS worlds, physics, device input,
prediction driver, presentation state, or gameplay systems. Network digests and
delta comparison remain exact over canonical bytes. Prediction tolerances apply
only after those bytes have been decoded into client simulation state.
