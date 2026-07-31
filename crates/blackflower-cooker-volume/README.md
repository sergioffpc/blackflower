# blackflower-cooker-volume

Host-only OpenVDB to NanoVDB cooking for Blackflower assets. The crate builds
the pinned OpenVDB core and its pinned Boost, oneTBB, and Blosc dependencies
into a private command-line tool. It consumes the repository-global zlib static
library prepared once by `cargo native build`. None of those dependencies are
linked into the game runtime.

The public Rust boundary accepts an authored `.vdb` path and an alphabetically
sorted, unique list of grid names. It preserves every directly supported grid
type, records bounds and active voxel counts, computes full NanoVDB checksums,
and emits one uncompressed `Codec::NONE` `.nvdb` payload. It does not quantize
values or preserve arbitrary DCC metadata.

OpenVDB sources may use raw, ZIP, or BLOSC storage. Unsupported selected grid
types, missing or duplicate grid names, invalid inputs, compressed NanoVDB
outputs, and outputs rejected by `blackflower-rendering-volumes` fail the cook.
