# blackflower-navigation

Safe Rust ownership and runtime queries over a statically linked
[RecastNavigation 1.6.0](https://github.com/recastnavigation/recastnavigation/releases/tag/v1.6.0).
The upstream source is pinned as the Git submodule `vendor/recastnavigation` at
commit `6dc1667f580357e8a2154c28b7867bea7e8ad3a7`.

The runtime library compiles Detour only. Recast navmesh generation belongs in
the offline content cooker; `DetourCrowd` and `DetourTileCache` are not part of
this initial binding surface. Generated declarations and all `unsafe` calls
remain private, behind the C ABI in `native/wrapper.h`.

## Checkout and prerequisites

Clone the repository and its submodules together:

```sh
git clone --recurse-submodules https://github.com/sergioffpc/blackflower.git
```

For an existing checkout:

```sh
git submodule update --init --recursive
```

The build needs CMake 3.20 or newer, a C++17 compiler, and the libclang shared
library used by bindgen. When libclang is outside the platform's normal search
path, point `LIBCLANG_PATH` at the directory containing `libclang.so`,
`libclang.dylib`, or `libclang.dll`.

To deliberately update RecastNavigation, fetch and check out a reviewed release
in the submodule, then commit the new submodule pointer:

```sh
git -C crates/blackflower-navigation/vendor/recastnavigation fetch --tags origin
git -C crates/blackflower-navigation/vendor/recastnavigation checkout v1.6.0
git add crates/blackflower-navigation/vendor/recastnavigation
```

## Runtime API

The asset cooker stores the tile bytes produced by `dtCreateNavMeshData` in a
versioned `.bfnav` container. The container retains the complete physical agent
profile, complete Recast settings, native version identity, canonical semantic
area table, tiled `dtNavMeshParams`, and ordered tile payloads. The runtime
validates this metadata before copying tile bytes into Detour-owned memory.
The runtime uses Detour's default 32-bit polygon references, so the cooker must
leave `DT_POLYREF64` disabled as well.

```rust,no_run
use blackflower_navigation::NavMeshAsset;
use bytes::Bytes;
use glam::Vec3A;

# fn cooked_navigation() -> Bytes { Bytes::new() }
# fn example() -> Result<(), blackflower_navigation::Error> {
let asset = NavMeshAsset::from_bytes(cooked_navigation())?;
let navmesh = asset.instantiate()?;
let query = navmesh.query()?;
let filter = asset.query_filter()?;
let search_extents = Vec3A::new(2.0, 4.0, 2.0);

let path = query.find_path(
    Vec3A::new(1.0, 0.0, 1.0),
    Vec3A::new(8.0, 0.0, 8.0),
    search_extents,
    &filter,
)?;

for point in path.points() {
    println!("{:?}: {}", point.kind(), point.position());
}
# Ok(())
# }
```

`NavMeshAsset::query_filter` compiles the baked policy to Detour's native
include flags and 64-entry cost array. Traversable polygons use flag bit 0;
blocked polygons carry no flags. Lua is not called while loading the asset or
while visiting polygons during a query.

`NavMesh` and `Query` are neither `Send` nor `Sync`. A query owns mutable search
state and borrows its mesh, preventing tile mutation or mesh destruction while
that query is alive.
