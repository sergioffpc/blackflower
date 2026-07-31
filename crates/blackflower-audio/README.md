# blackflower-audio

Facade for Blackflower's client audio stack.

The crate exposes the safe APIs implemented by:

- `blackflower-audio-media`, which owns 48 kHz `.bfaudio`, standard Ogg/Opus,
  source-less `.bfsound`, and the runtime `AudioLibrary`;
- `blackflower-audio-playback`, which keeps Kira and CPAL private while owning
  device mixing, Steam Audio HRTF tracks, and voice policy;
- `blackflower-audio-spatial`, which owns the statically linked Steam Audio
  integration and HRTF processing;
- `blackflower-audio-voice`, which owns the statically linked Opus codec
  integration.

Native libraries, generated bindings and `unsafe` operations remain private to
the crate that owns each integration. This facade does not compile or link
native code directly.

```rust
use blackflower_audio::{media, playback, spatial, voice};

assert_eq!(media::AUDIO_SAMPLE_RATE, 48_000);
assert_eq!(playback::KIRA_VERSION, "0.12.2");
assert_eq!(spatial::STEAM_AUDIO_VERSION, (4, 8, 1));
assert_eq!(voice::OPUS_VERSION, (1, 5, 2));
```
