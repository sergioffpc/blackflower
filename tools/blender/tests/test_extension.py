"""Integration tests for the glTF hooks with a minimal Blender API stub."""

from __future__ import annotations

import importlib.util
from pathlib import Path
from types import ModuleType, SimpleNamespace
import sys
import unittest


PACKAGE_ROOT = Path(__file__).parents[1] / "blackflower_gltf_metadata"


def _load_extension():
    bpy = ModuleType("bpy")
    bpy_props = ModuleType("bpy.props")
    bpy_types = ModuleType("bpy.types")

    class Object:
        pass

    class Action:
        pass

    class Material:
        pass

    class Panel:
        pass

    class PropertyGroup:
        pass

    bpy_props.BoolProperty = lambda **kwargs: None
    bpy_props.EnumProperty = lambda **kwargs: None
    bpy_props.FloatProperty = lambda **kwargs: None
    bpy_props.PointerProperty = lambda **kwargs: None
    bpy_props.StringProperty = lambda **kwargs: None
    bpy_types.Object = Object
    bpy_types.Action = Action
    bpy_types.Material = Material
    bpy_types.Scene = type("Scene", (), {})
    bpy_types.Panel = Panel
    bpy_types.PropertyGroup = PropertyGroup
    bpy.types = bpy_types
    bpy.props = bpy_props
    bpy.context = SimpleNamespace(
        scene=SimpleNamespace(
            blackflower_gltf_export=SimpleNamespace(enabled=True),
            render=SimpleNamespace(fps=24, fps_base=1),
            frame_start=0,
            frame_end=100,
        )
    )
    bpy.utils = SimpleNamespace(
        register_class=lambda class_type: None,
        unregister_class=lambda class_type: None,
    )
    sys.modules["bpy"] = bpy
    sys.modules["bpy.props"] = bpy_props
    sys.modules["bpy.types"] = bpy_types

    spec = importlib.util.spec_from_file_location(
        "blackflower_gltf_metadata",
        PACKAGE_ROOT / "__init__.py",
        submodule_search_locations=[str(PACKAGE_ROOT)],
    )
    module = importlib.util.module_from_spec(spec)
    sys.modules[spec.name] = module
    assert spec.loader is not None
    spec.loader.exec_module(module)
    return module, Object, Material


extension, BlenderObject, BlenderMaterial = _load_extension()


class ExportHookTests(unittest.TestCase):
    def test_blender_property_annotations_are_evaluated(self):
        self.assertNotIsInstance(
            extension.BlackflowerExportSettings.__annotations__["enabled"],
            str,
        )
        self.assertNotIsInstance(
            extension.BlackflowerNodeMetadata.__annotations__["kind"],
            str,
        )

    def test_action_hook_uses_exporter_range_and_slide(self):
        blender_object = BlenderObject()
        action = SimpleNamespace(
            name="Walk",
            frame_range=(10, 34),
            pose_markers=[
                SimpleNamespace(name="start", frame=10),
                SimpleNamespace(name="middle", frame=22),
            ],
            blackflower_animation_metadata=SimpleNamespace(
                looping=True,
                additive_enabled=False,
                additive_reference="animation",
                root_motion_enabled=True,
                root_motion_joint="Root",
                translation_x=True,
                translation_y=False,
                translation_z=True,
                rotation_x=False,
                rotation_y=True,
                rotation_z=False,
                root_motion_reference="skeleton",
                remove_from_pose=True,
                loop_correction=True,
            ),
        )
        target_id = "armature-uuid"
        export_settings = {
            "gltf_animation_mode": "ACTIONS",
            "gltf_merge_animation": "ACTION",
            "gltf_fps": 24,
            "vtree": SimpleNamespace(
                nodes={
                    target_id: SimpleNamespace(blender_object=blender_object),
                }
            ),
            "ranges": {
                target_id: {
                    "Walk": {"start": 10, "end": 34},
                }
            },
            "slide": {target_id: {"Walk": 10}},
        }
        gltf_animation = SimpleNamespace(extras={"vendor": True})
        action_data = SimpleNamespace(action=action)

        user_extension = extension.glTF2ExportUserExtension()
        user_extension.animation_action_hook(
            gltf_animation,
            blender_object,
            action_data,
            export_settings,
        )

        self.assertEqual(gltf_animation.extras["vendor"], True)
        self.assertEqual(
            gltf_animation.extras["blackflower"]["markers"],
            [
                {"name": "start", "time_seconds": 0.0},
                {"name": "middle", "time_seconds": 0.5},
            ],
        )
        self.assertTrue(gltf_animation.extras["blackflower"]["loop"])
        self.assertTrue(
            gltf_animation.extras["blackflower"]["root_motion"]["enabled"]
        )

    def test_ambiguous_animation_mode_is_rejected(self):
        action = SimpleNamespace(
            name="Walk",
            frame_range=(0, 24),
            pose_markers=[SimpleNamespace(name="event", frame=12)],
        )
        user_extension = extension.glTF2ExportUserExtension()

        with self.assertRaisesRegex(extension.MetadataError, "Actions mode"):
            user_extension.animation_action_hook(
                SimpleNamespace(extras=None),
                BlenderObject(),
                SimpleNamespace(action=action),
                {
                    "gltf_animation_mode": "NLA_TRACKS",
                    "gltf_merge_animation": "NLA_TRACK",
                },
            )

    def test_node_hook_exports_typed_identity(self):
        blender_object = BlenderObject()
        blender_object.blackflower_node_metadata = SimpleNamespace(
            enabled=True,
            kind="spawn_point",
            identifier="base_north",
        )
        gltf_node = SimpleNamespace(extras=None)

        user_extension = extension.glTF2ExportUserExtension()
        user_extension.gather_node_hook(gltf_node, blender_object, {})

        self.assertEqual(
            gltf_node.extras,
            {
                "blackflower": {
                    "schema": 1,
                    "node": {
                        "kind": "spawn_point",
                        "id": "base_north",
                    },
                }
            },
        )

    def test_node_hook_exports_navigation_policy(self):
        blender_object = BlenderObject()
        blender_object.blackflower_node_metadata = SimpleNamespace(
            enabled=False,
            kind="",
            identifier="floor_main",
            navigation_role="surface",
            navigation_area_key="ground",
            navigation_direction="bidirectional",
            navigation_radius=0.5,
        )
        gltf_node = SimpleNamespace(extras=None)

        extension.glTF2ExportUserExtension().gather_node_hook(
            gltf_node,
            blender_object,
            {},
        )

        self.assertEqual(gltf_node.extras["blackflower"]["schema"], 1)
        self.assertEqual(
            gltf_node.extras["blackflower"]["navigation"],
            {"role": "surface", "area_key": "ground"},
        )

    def test_node_hook_exports_probe_volume_without_recipe(self):
        blender_object = BlenderObject()
        blender_object.blackflower_node_metadata = SimpleNamespace(
            enabled=False,
            kind="",
            identifier="ground_floor_probes",
            navigation_role="none",
            acoustics_kind="probe_volume",
            acoustic_geometry_class="static",
            acoustic_zone="ground_floor",
        )
        gltf_node = SimpleNamespace(extras=None)

        extension.glTF2ExportUserExtension().gather_node_hook(
            gltf_node,
            blender_object,
            {},
        )

        self.assertEqual(
            gltf_node.extras["blackflower"],
            {
                "schema": 1,
                "node": {
                    "kind": "acoustic_probe_volume",
                    "id": "ground_floor_probes",
                },
                "acoustics": {
                    "kind": "probe_volume",
                    "zone": "ground_floor",
                },
            },
        )

    def test_node_hook_combines_navigation_and_static_acoustics(self):
        blender_object = BlenderObject()
        blender_object.blackflower_node_metadata = SimpleNamespace(
            enabled=False,
            kind="",
            identifier="floor_main",
            navigation_role="surface",
            navigation_area_key="ground",
            navigation_direction="bidirectional",
            navigation_radius=0.5,
            acoustics_kind="geometry",
            acoustic_geometry_class="static",
            acoustic_zone="",
        )
        gltf_node = SimpleNamespace(extras=None)

        extension.glTF2ExportUserExtension().gather_node_hook(
            gltf_node,
            blender_object,
            {},
        )

        blackflower = gltf_node.extras["blackflower"]
        self.assertEqual(blackflower["node"]["kind"], "acoustic_geometry")
        self.assertEqual(blackflower["navigation"]["role"], "surface")
        self.assertEqual(blackflower["acoustics"]["class"], "static")

    def test_material_hook_exports_acoustic_asset(self):
        material = BlenderMaterial()
        material.blackflower_material_metadata = SimpleNamespace(
            acoustic_material="acoustics/materials/concrete",
        )
        gltf_material = SimpleNamespace(extras={"vendor": True})

        extension.glTF2ExportUserExtension().gather_material_hook(
            gltf_material,
            material,
            {},
        )

        self.assertEqual(gltf_material.extras["vendor"], True)
        self.assertEqual(
            gltf_material.extras["blackflower"],
            {
                "schema": 1,
                "acoustics": {
                    "material": "acoustics/materials/concrete",
                },
            },
        )


if __name__ == "__main__":
    unittest.main()
