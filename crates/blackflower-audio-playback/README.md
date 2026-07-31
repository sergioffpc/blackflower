# blackflower-audio-playback

`blackflower-audio-playback` is the presentation-side audio engine. Its public
API exposes Blackflower media, events, and voice IDs while keeping Kira, CPAL,
and device handles private.

Kira owns mixing and device output. Ogg/Opus decoding is performed by Kira's
streaming worker and ring buffer. The audio callback does not perform file I/O,
decoding, allocation, locking, logging, acoustic simulation, or telemetry.

Two-dimensional sounds share one mixer track. HRTF voices use an ordinary Kira
track with panning disabled and a custom Steam Audio mono-to-stereo effect;
Kira's spatial panner is deliberately not used for those voices.
