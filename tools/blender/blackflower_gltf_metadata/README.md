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

## Typed map authoring

The extension exports only glTF or GLB. It never creates or rewrites
`map.toml`; that manifest remains an explicit, reviewable source file beside
the exported geometry.

The repository layout and manifest are strict:

```text
assets/source/maps/arena/map.toml
assets/source/maps/arena/arena.glb
```

```toml
schema = 1
id = "maps/arena"
source = "arena.glb"
scene = "Arena"
```

`cargo xtask assets check` derives `maps/arena` from the directory, opens the
contained `.gltf` or `.glb`, selects `Arena` exactly, and validates the complete
typed map boundary. There is no cooked map format or runtime loading contract
in this stage.

Select an Object or Empty and open **Object Properties > Blackflower
Metadata**. Enable **Blackflower Map Node**, assign a required stable
lower-snake-case **ID**, and select exactly one primary role:

- `geometry`, with independent render, collision, navigation, and acoustic
  uses;
- `spawn_point`, `prefab_instance`, `volume_instance`, or `trigger_volume`;
- `navigation_anchor` or `navigation_link`;
- `acoustic_zone` (identity, bounds, or probes) or `acoustic_portal`;
- `audio_emitter`.

Geometry, trigger volumes, portals, and acoustic bounds/probes use Mesh
objects. Transform-only roles and acoustic zone identities use Empty objects.
Links, probe bounds, and portals reference other enabled map objects through
Blender object pickers; the exporter resolves their stable IDs and validates
the complete exported map before writing it.

All map metadata uses the unreleased schema 1. For example:

```json
{
  "name": "North Spawn",
  "extras": {
    "blackflower": {
      "schema": 1,
      "node": {"id": "base_north", "role": "spawn_point"},
      "spawn_point": {"set": "players", "weight": 1.0}
    }
  }
}
```

One geometry node can feed multiple projections without acquiring multiple
primary roles:

```json
{
  "schema": 1,
  "node": {"id": "floor_main", "role": "geometry"},
  "geometry": {
    "render": true,
    "collision": true,
    "navigation": "surface",
    "acoustic_class": "static"
  }
}
```

In **Material Properties > Blackflower Surface**, a material may reference a
physics material asset, a navigation area key, and an acoustic material asset.
The coefficients, area costs, agent settings, probe placement recipe, and bake
quality remain in their domain manifests and the global cooking profile; they
are not duplicated in Blender.

## License

The Blender extension source in this directory is licensed under
GPL-2.0-or-later. The rest of Blackflower remains under the repository's MIT
license.
