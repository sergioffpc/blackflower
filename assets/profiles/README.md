# Cooking profiles

Each direct `.toml` child defines one complete cooking profile. The filename
stem is the profile name passed to `cargo xtask assets cook --profile`; there is
no duplicate `name` field inside the document. Settings belong to the selected
cook, never to individual assets.

Profile schema 1 configures Luau and shader compilation. The repository defines
two profiles:

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
```

The intended behavior is:

- `debug`: keeps Luau variable metadata and standard shader debug information,
  with compilation optimized for debugging.
- `release`: optimizes Luau and shaders while retaining Luau line information
  for useful runtime locations and stack traces.

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

Both Luau profiles emit type information for modules marked with `--!native`.
The runtime consumes it only when native codegen is explicitly enabled with an
executable-memory budget. Luau coverage instrumentation is always disabled by
the cooker and is not a profile setting.

Profiles are strict: missing settings, unknown fields, unsupported values, and
non-portable filenames are rejected. The cooker hashes the canonical semantic
profile rather than its TOML bytes, so formatting and comments do not change
`ProfileHash`. Every package records the selected profile name and hash. All
packages opened in one layered store must carry the same profile identity.
