# Blackflower authoritative acoustics

`blackflower-acoustics` owns deterministic, server-authoritative acoustic
propagation. It deliberately has no Steam Audio, CPAL, Kira, physics-world, or
network transport dependency.

Geometry is quantized to millimetres, material coefficients use Q0.16, asset
payloads are canonical JSON inside checksummed versioned containers, and all
runtime ordering uses stable numeric identifiers. Steam Audio remains a client
presentation backend and cannot affect gameplay audibility.

The crate defines the shared `.bfacmat`, `.bfactpl`, and `.bfacpfb` formats and
the simulation-only `.bfacsim` and `.bfacprf` formats.
