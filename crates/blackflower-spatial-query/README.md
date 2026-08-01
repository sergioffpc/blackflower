# Blackflower spatial queries

`blackflower-spatial-query` owns Blackflower's provider-neutral triangle-scene
and segment-query API. Its private native backend is the globally pinned Embree
4.4.1 source at `../../vendor/embree`. The repository-level `cargo native build`
step compiles Embree once; this crate only compiles its small C++ wrapper and
links the matching shared static archives.

The crate builds immutable committed scenes, returns closest or bounded
canonically ordered surface crossings, and keeps native handles and Embree IDs
behind safe Rust types. Domain crates map geometry and primitive IDs to their
own materials and gameplay rules; this crate deliberately contains no audio,
rendering, physics, ECS, or networking policy.

Embree builds and traverses the acceleration structures. Scene construction and
mutation happen outside queries, and committed scenes support concurrent
read-only queries.
