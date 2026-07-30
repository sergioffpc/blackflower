# Cooking profiles

Each direct `.toml` child defines one complete cooking profile. The filename
stem is the profile name passed to `cargo xtask assets cook --profile`; there is
no duplicate `name` field inside the document. Settings belong to the selected
cook, never to individual assets.

Profile schema 1 configures Luau, shader, and texture cooking. Development keeps
this schema at `1`; only the release process advances it. The repository
defines two profiles:

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

Both Luau profiles emit type information for modules marked with `--!native`.
The runtime consumes it only when native codegen is explicitly enabled with an
executable-memory budget. Luau coverage instrumentation is always disabled by
the cooker and is not a profile setting.

Profiles are strict: missing settings, unknown fields, unsupported values, and
non-portable filenames are rejected. The cooker hashes the canonical semantic
profile rather than its TOML bytes, so formatting and comments do not change
`ProfileHash`. Every package records the selected profile name and hash. All
packages opened in one layered store must carry the same profile identity.
