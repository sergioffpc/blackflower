# blackflower-audio

Pure Rust facade for Blackflower's client audio stack.

The crate exposes the safe APIs implemented by:

- `blackflower-audio-spatial`, which owns the statically linked Steam Audio
  integration and HRTF processing;
- `blackflower-audio-voice`, which owns the statically linked Opus codec
  integration.

Native libraries, generated bindings and `unsafe` operations remain private to
the crate that owns each integration. This facade does not compile or link
native code directly.

```rust
use blackflower_audio::{spatial, voice};

assert_eq!(spatial::STEAM_AUDIO_VERSION, (4, 8, 1));
assert_eq!(voice::OPUS_VERSION, (1, 5, 2));
```
