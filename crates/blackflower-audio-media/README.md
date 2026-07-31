# blackflower-audio-media

`blackflower-audio-media` owns the deterministic audio boundary shared by the
asset cooker and presentation runtime:

- authored mono/stereo WAV or FLAC is decoded and resampled to 48 kHz;
- short clips are stored as little-endian PCM16 in `.bfaudio`;
- streams are stored as standard Ogg/Opus with deterministic encoder settings;
- source-less `.bfsound` events reference media assets and carry playback policy.

The crate performs no device I/O. Streaming decode is pull-based so Kira can
run it on its worker thread instead of the real-time audio callback.
