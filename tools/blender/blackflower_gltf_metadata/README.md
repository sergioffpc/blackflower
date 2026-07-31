# Blackflower glTF Metadata for Blender

This Blender extension exports Blackflower-owned authoring metadata directly
into glTF 2.0 `extras`. It does not create a sidecar manifest and it does not
require the glTF exporter's **Custom Properties** option.

## Install

Build the installable archive from the repository root:

```sh
python3 tools/blender/build_blackflower_gltf_metadata.py
```

In Blender 4.2 or newer, open **Edit > Preferences > Add-ons**, choose
**Install from Disk**, and select:

```text
target/blender/blackflower_gltf_metadata-0.1.0.zip
```

The extension uses the user-extension hooks provided by Blender's bundled glTF
2.0 exporter. Keep **Blackflower Metadata** enabled in the glTF export dialog.

## Animation policy and markers

Select the Action in the Dope Sheet's **Action Editor**, then open the
**Blackflower** sidebar. The **Blackflower Animation** panel authors:

- loop playback;
- additive conversion and its animation or skeleton reference;
- root-motion extraction, exact joint, translation and rotation axes,
  reference, removal from the pose, and loop correction.

Markers remain Action-local **Pose Markers**:

1. Open the Dope Sheet and switch it to **Action Editor**.
2. Select the Action that becomes the glTF animation.
3. Add and name Pose Markers at the intended frames.
4. Export animation mode **Actions**. Merge mode **None** and **Action** are
   supported.

The extension resolves the effective range and timestamp slide calculated by
the glTF exporter, converts the marker frames using the scene FPS/FPS base, and
emits IEEE-754 `f32` seconds:

```json
{
  "animations": [
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
  ]
}
```

The add-on writes every field explicitly, including disabled settings; it does
not create a parallel animation manifest. Export fails when a marker is outside
the effective clip range, a joint or axis selection is invalid, another custom
property already owns `extras.blackflower`, or an animation/merge mode could
change the Action-local timeline ambiguously.

## Manual smoke test

Blender is not part of the current CI image. Before releasing the add-on,
install the built ZIP in Blender 4.2 or newer, author one Action with every
animation option and markers at the first and last frames, export glTF and GLB,
and inspect `animations[].extras.blackflower`. Re-export once and confirm the
metadata is identical. The repository's Python stubs cover the same hook and
serialization paths automatically.

## Model and level nodes

Select an Object or Empty and open **Object Properties > Blackflower
Metadata**. Enable the node and enter:

- **Kind**: required stable lower snake case type, such as `spawn_point`.
- **ID**: optional stable identifier within the source asset.

The exported glTF node receives:

```json
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
```

## Navigation geometry

In the same Object Properties panel, choose a **Navigation Role**:

- **Surface** rasterizes triangle geometry using an area key declared in the
  navigation asset's `asset.toml`.
- **Obstacle** rasterizes triangle geometry as blocked spans.
- **Off-mesh Link** exports one line primitive with exactly two indexed
  endpoints, an area key, direction, and positive endpoint radius.

Every navigation object requires a stable **ID**. Navigation metadata uses
strict node schema 1:

```json
{
  "extras": {
    "blackflower": {
      "schema": 1,
      "node": {
        "kind": "navigation_surface",
        "id": "floor_main"
      },
      "navigation": {
        "role": "surface",
        "area_key": "ground"
      }
    }
  }
}
```

Area costs, traversal permissions, physical agent dimensions, and Recast build
settings are not duplicated in Blender or Lua. They live exclusively in the
navigation `asset.toml`.

## Static acoustics

The extension version is kept equal to the workspace project version. In
**Object Properties > Blackflower Metadata**, choose an **Acoustic Role**:

- **Geometry**, then classify it as `static`, `dynamic_rigid`,
  `dynamic_state`, or `ignored`. Static-scene cooking consumes only `static`.
- **Zone** for a stable acoustic zone ID.
- **Probe Volume** for a bounded mesh object and its containing zone. Author a
  cube or other mesh whose local bounds define the volume; the cooker uses the
  full world transform.

One mesh may be both a navigation surface/obstacle and acoustic geometry; the
extension emits both schema-1 policies on that node.

Every acoustic object requires a stable **ID**. A probe volume exports only its
identity and zone:

```json
{
  "extras": {
    "blackflower": {
      "schema": 1,
      "node": {
        "kind": "acoustic_probe_volume",
        "id": "ground_floor_probes"
      },
      "acoustics": {
        "kind": "probe_volume",
        "zone": "ground_floor"
      }
    }
  }
}
```

Individual probes are not authored in Blender. `generation`,
`spacing_meters`, and `height_meters` live in the probe batch's `asset.toml`;
the cooker generates probes inside the selected volume.

In **Material Properties > Blackflower Acoustics**, map each material used by
static geometry to a portable acoustic material ID:

```json
{
  "extras": {
    "blackflower": {
      "schema": 1,
      "acoustics": {
        "material": "acoustics/materials/concrete"
      }
    }
  }
}
```

Absorption, scattering, and transmission coefficients are declared once in
the acoustic-scene manifest, not duplicated in Blender.

## License

The Blender extension source in this directory is licensed under
GPL-2.0-or-later. The rest of Blackflower remains under the repository's MIT
license.
