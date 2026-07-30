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
target/blender/blackflower_gltf_metadata-0.2.0.zip
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

This first node schema deliberately contains only identity. Domain-specific
map, physics, navigation, and acoustic fields should be added as typed schema
revisions with matching cooker support, not as an arbitrary property bag.

## License

The Blender extension source in this directory is licensed under
GPL-2.0-or-later. The rest of Blackflower remains under the repository's MIT
license.
