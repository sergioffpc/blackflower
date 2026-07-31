# Cooking profiles

Each direct `.toml` child defines one complete cooking profile. The filename
stem is the profile name passed to `cargo xtask assets cook --profile`; there is
no duplicate `name` field inside the document. Settings belong to the selected
cook, never to individual assets.

Profile schema 1 configures Luau, shader, texture, static mesh, animation,
audio, and static-acoustics cooking. Model hierarchy cooking and lossless
OpenVDB-to-NanoVDB conversion have no profile settings.
Development keeps this schema at `1`; only the release process advances it.
The repository defines two profiles:

```toml
# debug.toml
schema = 1

[scripting.luau]
optimization = "baseline"
debug = "full"
type_info = "native_modules"

[shaders]
target = "spirv"
capability = "spirv_1_5"
optimization = "none"
debug = "standard"

[textures]
ldr_encoding = "uastc"
hdr_encoding = "rgba16f"
quality = "fast"
zstd_level = 3
generate_mipmaps = true

[meshes]
lod_triangle_percents = [50, 25, 12]
lod_target_error = 0.01
optimize_overdraw = true
overdraw_threshold = 1.05
lock_borders = true

[animations]
sampling_rate_hz = 0.0
iframe_interval_seconds = 10.0
optimize = true
optimization_tolerance = 0.001
optimization_distance = 0.1
root_motion_tolerance = 0.001

[audio]
sample_rate = 48000
opus_frame_ms = 20
opus_complexity = 10
opus_mono_bitrate = 64000
opus_stereo_bitrate = 128000

[acoustics]
reflection_rays = 1024
diffuse_samples = 64
bounces = 4
simulated_duration_seconds = 1.0
saved_duration_seconds = 0.5
ambisonic_order = 1
bake_threads = 1
ray_batch_size = 64
irradiance_min_distance_meters = 0.1
bake_batch_size = 1
path_samples = 16
path_radius_meters = 0.5
path_visibility_threshold = 0.5
path_visibility_range_meters = 50.0
path_range_meters = 100.0
```

```toml
# release.toml
schema = 1

[scripting.luau]
optimization = "aggressive"
debug = "line_info"
type_info = "native_modules"

[shaders]
target = "spirv"
capability = "spirv_1_5"
optimization = "high"
debug = "none"

[textures]
ldr_encoding = "uastc"
hdr_encoding = "rgba16f"
quality = "high"
zstd_level = 15
generate_mipmaps = true

[meshes]
lod_triangle_percents = [50, 25, 12]
lod_target_error = 0.01
optimize_overdraw = true
overdraw_threshold = 1.05
lock_borders = true

[animations]
sampling_rate_hz = 0.0
iframe_interval_seconds = 10.0
optimize = true
optimization_tolerance = 0.001
optimization_distance = 0.1
root_motion_tolerance = 0.001

[audio]
sample_rate = 48000
opus_frame_ms = 20
opus_complexity = 10
opus_mono_bitrate = 64000
opus_stereo_bitrate = 128000

[acoustics]
reflection_rays = 16384
diffuse_samples = 1024
bounces = 32
simulated_duration_seconds = 2.0
saved_duration_seconds = 1.0
ambisonic_order = 2
bake_threads = 1
ray_batch_size = 256
irradiance_min_distance_meters = 0.1
bake_batch_size = 1
path_samples = 64
path_radius_meters = 0.5
path_visibility_threshold = 0.5
path_visibility_range_meters = 100.0
path_range_meters = 500.0
```

The intended behavior is:

- `debug`: keeps Luau variable metadata and standard shader debug information
  with compilation optimized for debugging, while using fast UASTC texture
  cooking.
- `release`: optimizes Luau and shaders, retains Luau line information, and
  uses high-quality single-threaded UASTC RDO texture cooking.

Luau supports:

- `optimization`: `none`, `baseline`, or `aggressive`
- `debug`: `none`, `line_info`, or `full`
- `type_info`: `native_modules` or `all_modules`

Shader cooking supports:

- `target`: `spirv`
- `capability`: `spirv_1_5`
- `optimization`: `none`, `default`, `high`, or `maximal`
- `debug`: `none`, `minimal`, `standard`, or `maximal`

`target` names the portable compiler output, not a graphics API or runtime
backend. Both profiles produce SPIR-V. The cooker validates that output with
Naga. A later runtime loader can pass it to wgpu, which selects and translates
to the active backend, including Metal on macOS, Vulkan, and Direct3D 12.

Texture cooking supports:

- `ldr_encoding`: `uastc`
- `hdr_encoding`: `rgba16f`
- `quality`: `fast` or `high`
- `zstd_level`: an integer from `1` through `22`
- `generate_mipmaps`: must be `true`

The cooker generates every mip level. Color mips are filtered in linear space,
normal mips are renormalized, and data and HDR mips remain linear. High quality
uses one BasisU worker with RDO multithreading disabled. KTX-Software documents
that BasisU output can still differ across platforms, so release packages must
come from the designated canonical cooking host.

Static mesh cooking supports:

- `lod_triangle_percents`: one through fifteen strictly decreasing percentages
  below the authored triangle count
- `lod_target_error`: meshoptimizer relative error limit in `(0, 1]`
- `optimize_overdraw`: whether to run overdraw optimization after vertex-cache
  optimization
- `overdraw_threshold`: permitted vertex-cache degradation from `1` through `2`
- `lock_borders`: whether simplification preserves topological borders

Each target is simplified sequentially from the previous LOD. Every resulting
LOD is then optimized for vertex cache, overdraw when enabled, and vertex
fetch. A target that cannot reduce the preceding LOD within the error limit is
omitted rather than duplicating geometry.

Model assets preserve the selected hierarchy and resolve explicit Mesh and
Volume attachments; those semantics live entirely in `asset.toml`. Volume
assets always preserve directly supported grid types, record bounds and active
voxel counts, compute full checksums, and emit uncompressed NanoVDB. Encoding,
quantization, statistics, and tolerance switches are intentionally not profile
dimensions.

Animation cooking supports:

- `sampling_rate_hz`: source sampling when `0.0`, otherwise a finite positive
  rate;
- `iframe_interval_seconds`: finite non-negative Ozz iframe interval;
- `optimize`: hierarchical key reduction;
- `optimization_tolerance`: positive hierarchy error tolerance;
- `optimization_distance`: positive distance used to measure hierarchy error;
- `root_motion_tolerance`: positive reduction tolerance for extracted
  root-motion tracks.

Per-joint optimization overrides are intentionally outside profile schema 1.
The profile controls compression and sampling; clip semantics remain in
`animation.extras.blackflower`.

Audio cooking supports one portable contract in every profile: mono/stereo
WAV or FLAC is resampled to 48 kHz, streams use Opus VBR with 20 ms frames and
complexity 10, and the target bitrates are 64 kbit/s for mono and 128 kbit/s
for stereo. These settings remain centralized in the selected profile; asset
manifests choose media semantics, not encoder overrides.

Static-acoustics profiles own Steam Audio bake quality: reflection rays,
diffuse samples, bounces, simulated/saved IR duration, ambisonic order,
threading, ray batches, irradiance distance, bake batches, and path visibility
and range parameters. `bake_threads` is deliberately `1` in both repository
profiles to make bake ordering stable. Probe `generation`, `spacing_meters`,
and `height_meters` remain asset-specific in the `.bfacprb` manifest.

Both Luau profiles emit type information for modules marked with `--!native`.
The runtime consumes it only when native codegen is explicitly enabled with an
executable-memory budget. Luau coverage instrumentation is always disabled by
the cooker and is not a profile setting.

Profiles are strict: missing settings, unknown fields, unsupported values, and
non-portable filenames are rejected. The cooker hashes the canonical semantic
profile rather than its TOML bytes, so formatting and comments do not change
`ProfileHash`. Every package records the selected profile name and hash. All
packages opened in one layered store must carry the same profile identity.
