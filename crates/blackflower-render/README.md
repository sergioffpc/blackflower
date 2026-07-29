# blackflower-render

Safe Rust ownership and CPU sampling for VDB assets, backed by the standalone
volume runtime included in
[OpenVDB 13.0.0](https://github.com/AcademySoftwareFoundation/openvdb/releases/tag/v13.0.0).
The upstream source is pinned as the Git submodule `vendor/openvdb` at commit
`7c03e1f084873cd1b3422c7ff7aec6ee681b3b38`.

The crate compiles only the standalone volume headers. It does not build the
OpenVDB core library and therefore does not add Boost, TBB, Blosc, or OpenEXR
to the game runtime. Generated declarations and every `unsafe` operation remain
private, behind the stable C ABI in `native/wrapper.h`.

The pinned runtime uses VDB binary format 32.9.0. The upstream and format
versions are exposed separately by `openvdb_version` and `vdb_version`.

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

To deliberately update OpenVDB, fetch and check out a reviewed release in the
submodule, then commit the new submodule pointer:

```sh
git -C crates/blackflower-render/vendor/openvdb fetch --tags origin
git -C crates/blackflower-render/vendor/openvdb checkout v13.0.0
git add crates/blackflower-render/vendor/openvdb
```

## Runtime API

The loader accepts raw VDB grid buffers and uncompressed `.nvdb` files.
Compressed ZIP and BLOSC files are rejected so the render runtime stays free
of system compression dependencies. The content cooker should emit raw or
`Codec::NONE` assets with checksums enabled.

VDB assets are trusted, versioned runtime content. They are not a sandboxed
format and must not be accepted directly from untrusted peers.

```rust,no_run
use blackflower_render::Vdb;
use glam::{DVec3, IVec3};

# fn cooked_volume() -> Vec<u8> { Vec::new() }
# fn example() -> Result<(), blackflower_render::Error> {
let asset = Vdb::from_bytes(&cooked_volume())?;
let grid = asset.grid(0).ok_or(blackflower_render::Error::InvalidAsset)?;
let density = grid
    .as_float()
    .ok_or(blackflower_render::Error::InvalidAsset)?;

let voxel = density.voxel(IVec3::new(4, 8, 12))?;
let filtered = density.sample_world(DVec3::new(1.25, 2.5, 3.75))?;

println!("active={}, value={}", voxel.is_active(), voxel.value());
println!("trilinear sample={filtered}");
# Ok(())
# }
```

The initial safe surface covers immutable metadata, transforms, scalar voxel
lookups, active-state queries, and trilinear world-space sampling for Float,
Fp4, Fp8, Fp16, and FpN grids. It deliberately excludes OpenVDB conversion,
grid mutation, file compression, and CUDA device ownership; those belong in
the offline content cooker or in a separately validated GPU integration.
