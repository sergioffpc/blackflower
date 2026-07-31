# blackflower-cooker-acoustics

Host-only Stage 8 cooker for immutable Steam Audio 4.8.1 data.

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

Dynamic rigid/state geometry, doors, portals, instancing, runtime simulation,
and audio-callback work are outside Stage 8.
