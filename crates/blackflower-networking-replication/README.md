# blackflower-networking-replication

`blackflower-networking-replication` turns sealed authoritative state into
client-specific component projections without owning transport or simulation.

The replication flow is:

1. apply public, owner, team, and global visibility before serialization;
2. project sealed state through stateful 512 m spherical interest;
3. quantize position, velocity, angles, and quaternion fields normatively;
4. compare components with the exact tick-and-digest applied baseline;
5. emit `Spawn`, `Update`, `RemoveComponent`, and `Forget` operations;
6. split canonical snapshot state into at most four transport chunks.

The crate performs no network I/O, compression, or mutation of the
authoritative simulation world.
