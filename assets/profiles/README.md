# Cooking profiles

Each direct `.toml` child defines one complete cooking profile. The filename
stem is the profile name passed to `cargo xtask assets cook --profile`; there is
no duplicate `name` field inside the document.

The initial schema configures only Luau. The repository defines two profiles:

```toml
# debug.toml
schema = 1

[scripting.luau]
optimization = "baseline"
debug = "full"
type_info = "native_modules"
```

The intended behavior is:

- `debug`: baseline optimization plus full debug metadata, suitable for source
  breakpoints, stepping, and inspection of named locals and upvalues.
- `release`: aggressive optimization plus line information, preserving useful
  runtime error locations and stack traces without full variable metadata.

Both profiles emit type information for modules marked with `--!native`.
The runtime consumes it only when native codegen is explicitly enabled with an
executable-memory budget.

```toml
# release.toml
schema = 1

[scripting.luau]
optimization = "aggressive"
debug = "line_info"
type_info = "native_modules"
```

Supported values are:

- `optimization`: `none`, `baseline`, or `aggressive`
- `debug`: `none`, `line_info`, or `full`
- `type_info`: `native_modules` or `all_modules`

Profiles are strict: missing settings, unknown fields, unsupported values, and
non-portable filenames are rejected. Asset manifests cannot override profile
settings. Luau coverage instrumentation is always disabled by the cooker and
is not a profile setting.

The cooker hashes the canonical semantic profile rather than its TOML bytes,
so formatting and comments do not change `ProfileHash`. Every package records
the selected profile name and hash. All packages opened in one layered store
must carry the same profile identity.

Future domain sections such as `[shaders]` will extend this file without moving
target-specific cooking decisions into individual asset manifests.
