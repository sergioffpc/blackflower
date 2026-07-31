# blackflower-gltf-metadata

Validation and strict extraction of Blackflower authoring metadata from glTF
2.0 JSON and GLB containers. This host-only crate owns the versioned
`extras.blackflower` namespace shared by domain cookers. It is never linked
into the game runtime.

`Document::open` is the cooker boundary. It rejects versions other than glTF
2.0, confines external buffers and images to the source directory, validates
and imports their contents with pinned `gltf` 1.4.1 on Linux, macOS, and
Windows, and enforces an explicit extension allowlist:

- `KHR_materials_unlit`;
- `KHR_mesh_quantization`;
- `KHR_texture_transform`.

`Document::from_bytes` performs in-memory `gltf` validation but cannot resolve
adjacent resources. Domain cookers must therefore use `Document::open`.

Metadata is attached to the glTF object that owns it. Playback, additive
conversion, root motion, and markers belong to an `animation` object:

```json
{
  "animations": [
    {
      "name": "Walk",
      "channels": [],
      "samplers": [],
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
            },
            {
              "name": "right_foot",
              "time_seconds": 0.71
            }
          ]
        }
      }
    }
  ]
}
```

Marker times use glTF seconds rather than normalized animation time. The cooker
sorts them deterministically, validates them against the imported Ozz duration,
and stores normalized ratios in `.bfanim`. Additive references are `animation`
or `skeleton`; root-motion references are `absolute`, `skeleton`, or
`animation`. Enabled root motion requires an exact joint and at least one
translation or rotation axis.

The `blackflower` object is strict and versioned. Unknown fields, unsupported
schemas, invalid names, negative or non-finite times, duplicate markers, and
ambiguous animation names are rejected. Axis lists cannot contain duplicates.
Unrelated third-party fields elsewhere in `extras` are ignored.

Model, level, navigation, and static-acoustic metadata reuse
`extras.blackflower` on the relevant glTF object instead of placing untyped
application data in a generic property bag.

The first node schema establishes typed identity for model and level objects:

```json
{
  "nodes": [
    {
      "name": "North Spawn",
      "extras": {
        "blackflower": {
          "schema": 1,
          "node": {
            "kind": "spawn_point",
            "id": "base_north"
          }
        }
      }
    }
  ]
}
```

`kind` is a required lower-snake-case domain type of at most 64 ASCII bytes.
`id` is an optional, non-empty stable identifier of at most 128 UTF-8 bytes.
Node names used for lookup must be unique. Schema 1 also accepts the strict
typed `navigation` and `acoustics` members implemented by their cookers.

Acoustic nodes use `acoustic_geometry`, `acoustic_zone`, or
`acoustic_probe_volume` identity kinds. Geometry is classified as `static`,
`dynamic_rigid`, `dynamic_state`, or `ignored`; Stage 8 imports only `static`.
A probe volume names its zone but never contains placement or bake quality.
Material objects use the same schema number and reference a portable acoustic
material ID through `acoustics.material`.
