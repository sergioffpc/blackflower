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

Metadata is attached to the glTF object that owns it. Animation markers belong
to an `animation` object:

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

Marker times use glTF seconds rather than normalized animation time. The
animation cooker validates them against the imported clip duration and converts
them to the normalized ratios consumed by `blackflower-animation`.

The `blackflower` object is strict and versioned. Unknown fields, unsupported
schemas, invalid names, negative or non-finite times, duplicate markers, and
ambiguous animation names are rejected. Unrelated third-party fields elsewhere
in `extras` are ignored.

Future model, level, physics, navigation, and acoustic metadata should reuse
`extras.blackflower` on the relevant glTF object and define a separate typed
schema rather than placing untyped application data in a generic property bag.

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
Node names used for lookup must be unique. Domain-specific map fields are not
part of schema 1; they will be added only alongside typed cooker support.
