# blackflower-audio-voice

Safe Rust ownership over the official
[Opus 1.5.2](https://github.com/xiph/opus/releases/tag/v1.5.2) reference
implementation.

The crate compiles Opus statically from pinned source and generates private C
bindings at build time. Raw declarations, variadic control calls and every
`unsafe` operation remain inside the private `ffi` module.

The initial safe API supports stateful mono and stereo floating-point encoding
and decoding, typed frame durations, variable bitrate, complexity, in-band FEC,
expected packet loss, discontinuous transmission and packet-loss concealment.
It does not implement microphone capture, device output, networking, jitter
buffering, Ogg encapsulation, multistream, projection, custom modes, DRED or
OSCE.

## Static native build

`cargo build` compiles `libopus.a` or `opus.lib` below Cargo's target directory.
No system Opus package, shared library or runtime codec installation is
required.

MSVC builds use CMake's `RelWithDebInfo` configuration so the native archive
links the same non-debug CRT selected by the Rust target. A target using
`crt-static` makes Opus select the corresponding static MSVC runtime.

The source is the Git submodule at `vendor/opus`, pinned to:

| Component | Revision | License |
| --- | --- | --- |
| Opus | `ddbe48383984d56acd9e1ab6a090c54ca6b735a6` (`v1.5.2`) | BSD-3-Clause |

Initialize it after cloning:

```sh
git submodule update --init --recursive
```

The build requires CMake, a C compiler and the libclang shared library used by
bindgen. When libclang is outside the platform's normal search path, set
`LIBCLANG_PATH` to the directory containing `libclang.so`, `libclang.dylib` or
`libclang.dll`.

## Safe API

```rust
use blackflower_audio_voice::{
    Application, Channels, Decoder, Encoder, Error, FrameDuration, SampleRate,
};

# fn example() -> Result<(), Error> {
let mut encoder = Encoder::new(
    SampleRate::Hz48K,
    Channels::Mono,
    Application::Voip,
)?;
encoder.set_bitrate(24_000)?;
encoder.set_inband_fec(true)?;

let input = vec![0.0; 960];
let mut packet = vec![0; 1_500];
let packet_len = encoder.encode(FrameDuration::Ms20, &input, &mut packet)?;

let mut decoder = Decoder::new(SampleRate::Hz48K, Channels::Mono)?;
let mut output = vec![0.0; 960];
let decoded = decoder.decode(&packet[..packet_len], &mut output)?;
assert_eq!(decoded, 960);
# Ok(())
# }
```
