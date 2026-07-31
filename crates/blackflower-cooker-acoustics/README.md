# blackflower-cooker-acoustics

Host-only Stage 8/9 cooker for immutable Steam Audio 4.8.1 presentation data
and deterministic pure-Rust authoritative data.

The cooker reads the original glTF/GLB, consumes only schema-1
`extras.blackflower` geometry classified as `static`, resolves every glTF
material through an explicit acoustic-material ID, and emits:

- `.bfacscn`: a checksummed serialized Steam Audio scene;
- `.bfacprb`: generated probes with base reflections, parametric reverb, and
  dynamic pathing layers.

Probe volumes are authored as ordinary bounded mesh objects. Their stable node
ID and zone are exported by the Blender extension. `generation`,
`spacing_meters`, and `height_meters` remain in the probe asset manifest; ray,
bounce, duration, and pathing quality remain in the selected cooking profile.

The same canonical `.bfacmat` coefficients feed Steam Audio scenes and the
authoritative cooker. Stage 9 additionally emits `.bfactpl` zone/portal state,
`.bfacpfb` rigid variants, millimetre-quantized `.bfacsim` geometry/BVH/path
data, and `.bfacprf` 20 ms spectral envelopes. Emission media is a cook-time
input only. Runtime state changes never invoke this crate, and audio-callback
work remains outside it.
