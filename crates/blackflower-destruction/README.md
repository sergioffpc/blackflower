# blackflower-destruction

`blackflower-destruction` owns the authoritative NVIDIA Blast integration for
destructible chunk and bond graphs. The public API uses Blackflower domain
types; Blast pointers, layouts, allocators, and fracture buffers remain behind
a private C ABI.

The first vertical surface supports:

- immutable assets built from authored chunks and bonds;
- one mutable family per destructible instance;
- direct chunk and bond fracture commands and resulting events;
- deterministic actor enumeration, visible-chunk queries, and actor splitting;
- `NvBlastExtStress` on supported x86-64 native targets.

Blast 5.0.6 is pinned through the shared `vendor/PhysX` submodule. The upstream
stress solver is x86 SIMD-only at this revision, so Apple Silicon and other
non-x86 targets retain Low Level destruction but report stress as unsupported.
The wrapper never runs upstream setup scripts from Cargo.

Jolt remains responsible for rigid bodies and contacts. This crate reports
chunk/bond topology changes for the simulation layer to materialize in Jolt;
it does not own physics bodies, ballistics, damage policy, rendering, or
replication.
