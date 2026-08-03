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
- `KHR_lights_punctual`;
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

Map authoring reuses `extras.blackflower` on the relevant glTF node or material
instead of placing untyped application data in a generic property bag.
`Document::map_metadata` selects exactly one named scene and returns its typed
nodes in stable glTF node-index order.

Every map node has a required map-local ID, one closed primary role, and
exactly the payload belonging to that role:

```json
{
  "nodes": [
    {
      "name": "North Spawn",
      "extras": {
        "blackflower": {
          "schema": 1,
          "node": {
            "id": "base_north",
            "role": "spawn_point"
          },
          "spawn_point": {"set": "players", "weight": 1.0}
        }
      }
    }
  ]
}
```

The schema-1 roles are `geometry`, `spawn_point`, `prefab_instance`,
`volume_instance`, `trigger_volume`, `navigation_anchor`, `navigation_link`,
`acoustic_zone`, `acoustic_portal`, and `audio_emitter`. Geometry is the only
role with combined domain uses. Links must target navigation anchors; probes
must target acoustic zone identities; portals must target two distinct zone
bounds and may name a geometry or prefab controller.

Material objects use the same schema and may contain `physics_material`,
`navigation_area`, and `acoustic_material` inside a strict `material` payload.
The map parser validates portable IDs, finite positive weights/radii, mesh vs
transform-only roles, duplicate IDs, and all cross-node references.
