# Blackflower authoritative acoustics

`blackflower-acoustics` owns deterministic, server-authoritative acoustic
propagation. It uses `blackflower-spatial-query` for immutable Embree scenes and
bounded segment queries, while retaining all material, routing, and audibility
policy. It deliberately has no Steam Audio, CPAL, Kira, physics-world, or
network transport dependency.

The crate also owns the stable `BFAD` v1 application codec that serializes its
acoustic propagation and live-voice delivery types. Transport treats these
payloads as opaque bytes.

Geometry is quantized to millimetres, material coefficients use Q0.16, asset
payloads are canonical TOML inside checksummed versioned containers, and all
runtime ordering uses stable numeric identifiers. Steam Audio remains a client
presentation backend and cannot affect gameplay audibility.

The crate defines the shared `.bfacmat`, `.bfactpl`, and `.bfacpfb` formats and
the simulation-only `.bfacsim` and `.bfacprf` formats.
