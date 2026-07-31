# blackflower-audio-spatial

Safe Rust ownership over the official
[Steam Audio SDK 4.8.1](https://github.com/ValveSoftware/steam-audio/releases/tag/v4.8.1).
The complete C API declarations are generated from the pinned `phonon.h` at
build time. Raw declarations, statically linked native calls and every `unsafe`
operation stay inside the private `ffi` module.

The safe API creates a context, manages committed acoustic scene geometry,
loads checksummed `.bfacscn` scenes and `.bfacprb` probe batches, uses Steam
Audio's built-in HRTF, and processes fixed-size mono PCM frames into
deinterleaved stereo with one stateful `BinauralEffect` per source. Device
output, decoding, mixing, and authoritative gameplay simulation remain outside
this crate. It also provides rigid `InstancedMesh` scene updates, allocation-free
`DirectEffect`/`PathEffect`, a lock-free triple parameter exchange, and an
off-callback dirty-zone `ReflectionSimulator`. Steam Audio reflections remain
presentation-only and cannot change audibility.

`Context::new` selects the statically linked Embree 4.4.1 ray tracer on x86-64
Linux, macOS and Windows, and on ARM64 Linux and macOS. Other targets retain
Steam Audio's built-in ray tracer. `Context::with_ray_tracer` provides an
explicit strict selection for tests, diagnostics, and benchmarks; requesting
Embree where it was not compiled returns `Error::RayTracerUnavailable`.

Steam Audio only permits serialization of scenes created with its built-in
ray tracer. Cooking therefore uses `Context::create_serializable_scene`, then
loads the resulting `.bfacscn` through `Context::load_acoustic_scene`; that
loaded scene uses the context's selected backend, including Embree. Calling
`Scene::to_acoustic_asset` on an Embree scene returns
`Error::SceneSerializationRequiresBuiltIn` instead of crossing the invalid
native API path.

The native acoustic containers use schema 1 and embed the exact Steam Audio 4.8.1
serialization behind Blackflower headers, counts, layer identifiers, and
BLAKE3 checksums:

- `.bfacscn`: immutable committed static scene;
- `.bfacprb`: deterministic probe order plus base reflections/parametric
  reverb and dynamic pathing layers;
- `.bfac`: ordered zone references plus the shared `.bfactpl` topology.

Parsing and native object creation are explicit worker/loading operations.
They are not permitted in an audio callback.

## Static native build

`cargo build` compiles the complete native audio stack from pinned source and
links it statically:

- Steam Audio 4.8.1 as `libphonon.a` or `phonon.lib`;
- Embree 4.4.1 on supported x86-64 and ARM64 targets. Steam Audio's ISPC
  reflection kernels use ISPC 1.31.0 on x86-64; ARM64 deliberately uses its
  portable C++ reflection simulator over the Embree scene;
- PFFFT as the fallback FFT implementation;
- libmysofa for SOFA HRTF data;
- the repository-global zlib source as libmysofa's compression dependency;
- FlatBuffers' `flatc` as a host-only schema compiler. FlatBuffers is
  header-only at runtime.

No precompiled Steam Audio SDK, `BLACKFLOWER_STEAM_AUDIO_LIBRARY`, shared
`phonon` library or runtime SDK installation is required. `Context::new`
calls the statically linked API directly.

The generated files and native archives are kept below Cargo's target
directory:

```text
target/<profile>/build/blackflower-audio-spatial-*/out/native/
├── flatbuffers/build/flatc
├── embree/build/.../libembree.a
├── libmysofa/build/.../libmysofa.a
├── pffft/build/libpffft.a
├── steam-audio/build/.../libphonon.a
└── zlib/build/libz.a
```

Windows produces the corresponding `.lib` files. These paths are Cargo build
artifacts and must not be copied into an application bundle.

The Steam Audio stack is static, while the operating-system and C/C++ runtime
follow the Rust target's normal policy. For example, a default macOS executable
still uses the system `libc++` and `libSystem`; a Windows target with
`crt-static` makes the native CMake builds use the matching static MSVC runtime.
MSVC dependencies use CMake's `RelWithDebInfo` configuration even for Cargo
debug builds so they link the same non-debug CRT selected by the Rust target
without enabling link-time code generation inside archives embedded by Cargo.

## Pinned source

All native sources are Git submodules at the exact revisions expected by Steam
Audio:

| Component | Revision | License |
| --- | --- | --- |
| Steam Audio | `0da18255cca520771f363ee01f100572b39a308e` (`v4.8.1`) | Apache-2.0 |
| Embree | `f590db83ef6559387df7f6d8725c34fb7acf851d` (`v4.4.1`) | Apache-2.0 |
| FlatBuffers | `6df40a2471737b27271bdd9b900ab5f3aec746c7` | Apache-2.0 |
| libmysofa | `dd315a8ec1fee7193d40e4a59b12c5590a4a918c` | BSD-3-Clause |
| PFFFT | `e0bf595c98ded55cc457a371c1b29c8cab552628` | BSD-3-Clause |
| zlib (global `vendor/zlib`) | `51b7f2abdade71cd9bb0e7a373ef2610ec6f9daf` | Zlib |

Initialize them after cloning:

```sh
git submodule update --init --recursive
```

The authoritative license texts remain in each submodule. Binary
redistribution must preserve the notices required by those licenses,
particularly Embree, libmysofa and PFFFT.

The build requires CMake, a C/C++ compiler and the libclang shared library used
by bindgen. When libclang is outside the platform's normal search path, set
`LIBCLANG_PATH` to the directory containing `libclang.so`, `libclang.dylib` or
`libclang.dll`.

Supported x86-64 builds additionally require the exact ISPC 1.31.0 compiler.
Steam Audio 4.8.1 requests ISPC 1.12 upstream; Blackflower patches that exact
version check to 1.31 and exercises Steam Audio's reflection kernels together
with Embree 4.4.1 in every x86-64 release job. Set `BLACKFLOWER_ISPC` to the
compiler executable or place it on `PATH`. CI installs the architecture-specific
official release archive after verifying its pinned SHA-256 digest; the same
helper is available locally on Linux and macOS x86-64/ARM64 and Windows
x86-64:

```sh
python3 .github/scripts/install-ispc.py --output /tmp/blackflower-ispc
```

The release workflow uses IntelLLVM 2025.0.4 for native C/C++ compilation on
Linux and Windows x86-64. Equivalent local builds can select an existing
oneAPI installation explicitly:

```sh
CC=icx CXX=icpx \
BLACKFLOWER_ISPC=/path/to/ispc \
cargo build --release --package blackflower-audio-spatial
```

On Windows use `icx-cl` for both `CC` and `CXX` and set
`CMAKE_GENERATOR=Ninja`. macOS uses AppleClang because IntelLLVM does not
support macOS. Rust sources continue to use `rustc`; these variables select
only the C/C++ compiler used by native dependencies.

Native builds compile the pinned `flatc` automatically. Cross-compilation
cannot run a target executable on the build host, so it additionally requires
`BLACKFLOWER_FLATC` to point to a host `flatc` built from the pinned
FlatBuffers revision.

## Safe HRTF API

```rust
use blackflower_audio_spatial::{
    AudioSettings, BinauralParams, Context, Error, RayTracerBackend, Vec3A,
};

# fn example() -> Result<(), Error> {
let settings = AudioSettings::new(48_000, 256)?;
let mut context = Context::new()?;
assert!(matches!(
    context.ray_tracer_backend(),
    RayTracerBackend::BuiltIn | RayTracerBackend::Embree
));
let hrtf = context.create_default_hrtf(settings)?;
let mut effect = context.create_binaural_effect(&hrtf)?;

let input = vec![0.0; 256];
let mut left = vec![0.0; 256];
let mut right = vec![0.0; 256];
effect.process_mono(
    BinauralParams::new(Vec3A::X)?,
    &input,
    &mut left,
    &mut right,
)?;
# Ok(())
# }
```

Steam Audio uses the same basis as Blackflower's
[engine coordinate system](../../docs/coordinate-system.md): positive x points
right, positive y points up and negative z points ahead. `BinauralParams`
expects a listener-relative direction and normalizes it before crossing the
FFI boundary.
