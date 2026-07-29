# blackflower-audio

Safe Rust ownership over the official
[Steam Audio SDK 4.8.1](https://github.com/ValveSoftware/steam-audio/releases/tag/v4.8.1).
The complete C API declarations are generated from the pinned `phonon.h` at
build time. Raw declarations, dynamic loading and every `unsafe` operation stay
inside the private `ffi` module.

The initial safe vertical slice creates a context, loads Steam Audio's built-in
HRTF and processes fixed-size mono PCM frames into deinterleaved stereo with one
stateful `BinauralEffect` per source. It deliberately does not yet implement
device output, decoding, mixing, scene geometry, occlusion or reflections.

## Vendored SDK

`vendor/steam-audio-sdk` is a Git submodule of the official Steam Audio
repository, pinned to tag `v4.8.1` (commit
`0da18255cca520771f363ee01f100572b39a308e`). Initialize it after cloning:

```sh
git submodule update --init --recursive
```

The upstream source repository contains the C API header but not the compiled
SDK libraries. Download `steamaudio_4.8.1.zip` from the
[official release](https://github.com/ValveSoftware/steam-audio/releases/tag/v4.8.1)
or build the SDK following Valve's instructions. Then point
`BLACKFLOWER_STEAM_AUDIO_LIBRARY` at the shared library for the current target:

```sh
export BLACKFLOWER_STEAM_AUDIO_LIBRARY=/path/to/libphonon.dylib
```

The build also needs the libclang shared library used by bindgen. When libclang
is outside the platform's normal search path, set `LIBCLANG_PATH` to the
directory containing `libclang.so`, `libclang.dylib` or `libclang.dll`.

`Context::new` loads `BLACKFLOWER_STEAM_AUDIO_LIBRARY` when set, otherwise it
asks the platform loader for `libphonon.so`, `libphonon.dylib` or `phonon.dll`.
A packaged application can use unsafe `Context::from_library_path` for an
authentic v4.8.1 library copied into the application bundle. The caller must
guarantee the library matches the pinned header ABI.

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
