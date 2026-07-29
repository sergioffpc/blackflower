# blackflower-audio

Safe Rust ownership over the official
[Steam Audio SDK 4.8.1](https://github.com/ValveSoftware/steam-audio/releases/tag/v4.8.1).
The complete C API declarations are generated from the pinned `phonon.h` at
build time. Raw declarations, statically linked native calls and every `unsafe`
operation stay inside the private `ffi` module.

The initial safe vertical slice creates a context, uses Steam Audio's built-in
HRTF and processes fixed-size mono PCM frames into deinterleaved stereo with one
stateful `BinauralEffect` per source. It deliberately does not yet implement
device output, decoding, mixing, scene geometry, occlusion or reflections.

## Static native build

`cargo build` compiles the complete native audio stack from pinned source and
links it statically:

- Steam Audio 4.8.1 as `libphonon.a` or `phonon.lib`;
- PFFFT as the fallback FFT implementation;
- libmysofa for SOFA HRTF data;
- zlib as libmysofa's compression dependency;
- FlatBuffers' `flatc` as a host-only schema compiler. FlatBuffers is
  header-only at runtime.

No precompiled Steam Audio SDK, `BLACKFLOWER_STEAM_AUDIO_LIBRARY`, shared
`phonon` library or runtime SDK installation is required. `Context::new`
calls the statically linked API directly.

The generated files and native archives are kept below Cargo's target
directory:

```text
target/<profile>/build/blackflower-audio-*/out/native/
├── flatbuffers/build/flatc
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

## Pinned source

All native sources are Git submodules at the exact revisions expected by Steam
Audio:

| Component | Revision | License |
| --- | --- | --- |
| Steam Audio | `0da18255cca520771f363ee01f100572b39a308e` (`v4.8.1`) | Apache-2.0 |
| FlatBuffers | `6df40a2471737b27271bdd9b900ab5f3aec746c7` | Apache-2.0 |
| libmysofa | `dd315a8ec1fee7193d40e4a59b12c5590a4a918c` | BSD-3-Clause |
| PFFFT | `e0bf595c98ded55cc457a371c1b29c8cab552628` | BSD-3-Clause |
| zlib | `51b7f2abdade71cd9bb0e7a373ef2610ec6f9daf` | Zlib |

Initialize them after cloning:

```sh
git submodule update --init --recursive
```

The authoritative license texts remain in each submodule. Binary
redistribution must preserve the notices required by those licenses,
particularly libmysofa and PFFFT.

The build requires CMake, a C/C++ compiler and the libclang shared library used
by bindgen. When libclang is outside the platform's normal search path, set
`LIBCLANG_PATH` to the directory containing `libclang.so`, `libclang.dylib` or
`libclang.dll`.

Native builds compile the pinned `flatc` automatically. Cross-compilation
cannot run a target executable on the build host, so it additionally requires
`BLACKFLOWER_FLATC` to point to a host `flatc` built from the pinned
FlatBuffers revision.

## Safe HRTF API

```rust
use blackflower_audio::{
    AudioSettings, BinauralParams, Context, Error, Vec3A,
};

# fn example() -> Result<(), Error> {
let settings = AudioSettings::new(48_000, 256)?;
let mut context = Context::new()?;
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

Steam Audio uses a right-handed coordinate system: positive x points right,
positive y points up and negative z points ahead. `BinauralParams` expects a
listener-relative direction and normalizes it before crossing the FFI boundary.
