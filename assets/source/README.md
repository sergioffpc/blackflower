# Asset sources

The cooker recursively discovers files named `asset.toml` below this
directory. Other files are inputs referenced by those manifests.

Opaque blobs pass through unchanged:

```toml
schema = 1
id = "fixtures/example"
kind = "blob"
audience = "shared"

[blob]
source = "example.bin"
```

Luau assets contain source only. Compiler settings come exclusively from the
selected file in `assets/profiles`:

```toml
schema = 1
id = "scripts/weapon_policy"
kind = "luau_bytecode"
audience = "simulation"

[luau]
source = "weapon_policy.luau"
```

The cooker rejects invalid UTF-8 and Luau compilation errors before publishing
a package. Packages contain the resulting bytecode, not the source text.

Shader assets contain Slang source plus the entry point and shader stage:

```toml
schema = 1
id = "shaders/basic"
kind = "shader_module"
audience = "presentation"

[shader]
source = "basic.slang"
entry_point = "vertex_main"
stage = "vertex"
```

Supported stages are `vertex`, `fragment`, and `compute`. Shader assets are
presentation-only. Target, capability, optimization, and debug settings come
exclusively from the selected cooking profile. The cooker compiles one entry
point to SPIR-V with the pinned Slang compiler, rejects imports and includes,
validates the result with Naga, and packages only the validated SPIR-V bytes.

Package composition has one canonical location:

```text
packages/<logical-name>/package.toml
```

For example, `--package pak000` reads `packages/pak000/package.toml`:

```toml
schema = 1
assets = ["fixtures/example"]
```

The cooker includes exactly those assets. Runtime relationships belong in
typed composite assets such as prefabs, materials, and scenes; they are not
authored as dependencies in `asset.toml`. There is no separate level manifest
or command-line composition override. IDs, package names, source containment,
and schemas are validated before a package is written.
