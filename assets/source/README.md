# Asset sources

The cooker recursively discovers files named `asset.toml` or
`*.asset.toml` below this directory. Named manifests allow several assets
beside one shared source file, such as one skeleton and multiple clips selected
from the same glTF. Other files are inputs referenced by those manifests.

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

Texture assets contain one authored PNG or OpenEXR image and its texel
semantics:

```toml
schema = 1
id = "textures/vehicle_albedo"
kind = "texture2d"
audience = "presentation"

[texture]
source = "vehicle_albedo.png"
semantic = "color_srgb"
```

Supported semantics are `color_srgb`, `normal_linear`, `data_linear`, and
`hdr_linear`. The first three require PNG; `hdr_linear` requires OpenEXR.
Textures are presentation-only. The selected profile owns encoding quality,
mipmap generation, and Zstandard compression. The package contains validated
KTX2 rather than the authored image. At runtime,
`blackflower-rendering-textures` selects BC, ASTC, ETC2/EAC, or an uncompressed
fallback from renderer capabilities.

Static mesh assets select one uniquely named glTF mesh:

```toml
schema = 1
id = "models/vehicle_body"
kind = "mesh"
audience = "presentation"

[mesh]
source = "vehicle.gltf"
mesh = "VehicleBody"
```

Meshes are presentation-only. The cooker preserves triangle primitives and
their glTF material indices as separate draw units, then runs vertex-cache,
optional overdraw, and vertex-fetch optimization. It generates the
profile-owned LOD chain with meshoptimizer and packages only the validated
runtime mesh. Authored glTF files are never rewritten.

This first static format supports `POSITION` plus optional `NORMAL`, `TANGENT`,
and `TEXCOORD_0`. It rejects non-triangle primitives, skinning attributes,
morph targets, colors, and additional texture-coordinate sets. External glTF
buffers participate in recipe identity, so changing geometry without changing
the `.gltf` document still forces a recook.

Model assets select one exact named glTF scene and explicitly attach Mesh or
Volume assets to named nodes:

```toml
schema = 1
id = "models/vehicle"
kind = "model"
audience = "presentation"

[model]
source = "vehicle.gltf"
scene = "Vehicle"

[[model.attachments]]
node = "Body"
asset = "models/vehicle_body"

[[model.attachments]]
node = "Exhaust"
asset = "volumes/exhaust"
```

The attachment kind is inferred from the referenced asset. The cooker
preserves the complete selected hierarchy, including unnamed nodes, in
depth-first source order. It preserves authored TRS or matrix representation
without decomposing or normalizing it. Only node names used by attachments
must be unique.

A node may have at most one Mesh attachment and any number of distinct Volume
attachments. Every glTF mesh referenced by the selected scene requires one
explicit Mesh attachment to the matching source and mesh selection. Cameras,
lights, skins, cycles, shared nodes, non-finite transforms, and non-normalized
quaternions fail the cook. Volume grid transforms remain local to the model
node.

Volume assets select one or more exact OpenVDB grid names:

```toml
schema = 1
id = "volumes/exhaust"
kind = "volume"
audience = "presentation"

[volume]
source = "exhaust.vdb"
grids = ["density", "temperature"]
```

Volumes are presentation-only. Grid names are canonicalized alphabetically
and duplicates are rejected. Authored OpenVDB files may use raw, ZIP, or BLOSC
storage. The host-only cooker preserves directly supported grid types without
quantization, keeps runtime metadata only, records bounds and active voxel
counts, computes full checksums, and emits uncompressed `Codec::NONE` NanoVDB.
There is no encoding or volume-quality setting in `asset.toml` or in the
cooking profile.

Skeleton and animation assets are presentation-only. A skeleton selects one
exact named skin and cooks it to `.bfskel`:

```toml
schema = 1
id = "characters/soldier/rig"
kind = "skeleton"
audience = "presentation"

[skeleton]
source = "soldier.gltf"
skin = "SoldierRig"
```

Each animation asset selects exactly one named clip and declares the skeleton
asset it requires:

```toml
schema = 1
id = "characters/soldier/walk"
kind = "animation_clip"
audience = "presentation"

[animation]
source = "soldier.gltf"
clip = "Walk"
skeleton = "characters/soldier/rig"
```

Place these in files such as `rig.asset.toml` and `walk.asset.toml` beside
`soldier.gltf`. Additional clip manifests select other animations from that
same source. Rig and animation sources may also be separate glTF or GLB files;
the declared skeleton identity is validated after both are cooked. Selecting a
clip in `package.toml` automatically includes its skeleton dependency.

glTF and GLB sources attach Blackflower-specific authoring data to the glTF
object that owns it. The shared `extras.blackflower` namespace is strict and
versioned; unrelated third-party extras remain outside that namespace.
`cargo xtask assets check` and every package cook validate all `.gltf` and
`.glb` files below this directory before publishing anything. External buffers
and images must use portable relative paths contained by the source file's
directory; remote URLs, absolute paths, traversal, missing resources, invalid
glTF 2.0 structure, and unsupported extensions are rejected.
Animation policy belongs to the matching glTF `animation` object:

```json
{
  "name": "Walk",
  "extras": {
    "blackflower": {
      "schema": 1,
      "loop": true,
      "additive": {
        "enabled": false,
        "reference": "animation"
      },
      "root_motion": {
        "enabled": true,
        "joint": "Root",
        "translation_axes": ["x", "z"],
        "rotation_axes": ["y"],
        "reference": "skeleton",
        "remove_from_pose": true,
        "loop_correction": true
      },
      "markers": [
        {
          "name": "left_foot",
          "time_seconds": 0.24
        }
      ]
    }
  }
}
```

Marker times use glTF seconds. Cooking validates them against the selected Ozz
clip duration and converts them to normalized runtime time. Loop, additive,
markers, and root motion are not duplicated in the asset manifest. The
complete metadata contract and validation rules live in
`crates/blackflower-gltf-metadata/README.md`.

The repository Blender extension authors Action Pose Markers and typed model or
level node identity without a sidecar file. Build and install it using the
instructions in `tools/blender/blackflower_gltf_metadata/README.md`.

Package composition has one canonical location:

```text
packages/<logical-name>/package.toml
```

For example, `--package pak000` reads `packages/pak000/package.toml`:

```toml
schema = 1
assets = ["fixtures/example"]
```

The cooker includes those explicitly selected assets plus typed mandatory
dependencies: the `.bfskel` named by each selected animation clip and every
Mesh or Volume attached by a selected model.
Other runtime relationships belong in typed composite assets such as prefabs,
materials, and scenes; arbitrary dependency lists are not accepted in
`asset.toml`. There is no separate level manifest or command-line composition
override. IDs, package names, source containment, and schemas are validated
before a package is written.
