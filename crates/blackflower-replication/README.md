# blackflower-replication

`blackflower-replication` turns sealed authoritative state into
client-specific snapshot deltas without owning transport or simulation state.

The replication flow is:

1. project a sealed `ReplicationSource` through a client's area of interest;
2. quantize the projected component fields with the shared protocol policy;
3. compare the quantized snapshot with the last acknowledged baseline;
4. retain sent snapshots until an acknowledgement promotes a new baseline;
5. hand the resulting delta to a transport-specific encoder.

The crate performs no network I/O, serialization, compression, or mutation of
the authoritative simulation world.
