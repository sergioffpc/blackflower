"""Tests for Blender-independent metadata serialization."""

from __future__ import annotations

import importlib.util
from pathlib import Path
import struct
import unittest


MODULE_PATH = (
    Path(__file__).parents[1] / "blackflower_gltf_metadata" / "metadata.py"
)
SPEC = importlib.util.spec_from_file_location("blackflower_metadata", MODULE_PATH)
metadata = importlib.util.module_from_spec(SPEC)
assert SPEC.loader is not None
SPEC.loader.exec_module(metadata)


class AnimationMetadataTests(unittest.TestCase):
    def test_markers_are_sorted_and_mapped_to_exporter_time(self):
        result = metadata.build_animation_metadata(
            [("right_foot", 18), ("left_foot", 6)],
            frame_start=0,
            frame_end=24,
            frames_per_second=24,
            time_origin_frame=0,
        )

        self.assertEqual(result["schema"], 1)
        self.assertFalse(result["loop"])
        self.assertEqual(
            result["markers"],
            [
                {"name": "left_foot", "time_seconds": 0.25},
                {"name": "right_foot", "time_seconds": 0.75},
            ],
        )

    def test_slid_timeline_uses_effective_origin(self):
        result = metadata.build_animation_metadata(
            [("start", 10), ("middle", 22)],
            frame_start=10,
            frame_end=34,
            frames_per_second=24,
            time_origin_frame=10,
        )

        self.assertEqual(
            result["markers"],
            [
                {"name": "start", "time_seconds": 0.0},
                {"name": "middle", "time_seconds": 0.5},
            ],
        )

    def test_marker_seconds_are_canonical_float32(self):
        result = metadata.build_animation_metadata(
            [("event", 1)],
            frame_start=0,
            frame_end=2,
            frames_per_second=30,
            time_origin_frame=0,
        )
        expected = struct.unpack("<f", struct.pack("<f", 1 / 30))[0]

        self.assertEqual(result["markers"][0]["time_seconds"], expected)

    def test_empty_action_exports_explicit_policy(self):
        result = metadata.build_animation_metadata(
            [],
            frame_start=0,
            frame_end=1,
            frames_per_second=24,
            time_origin_frame=0,
        )
        self.assertEqual(result["markers"], [])
        self.assertEqual(
            result["additive"],
            {"enabled": False, "reference": "animation"},
        )

    def test_complete_animation_policy_is_serialized(self):
        result = metadata.build_animation_metadata(
            [],
            frame_start=0,
            frame_end=24,
            frames_per_second=24,
            time_origin_frame=0,
            looping=True,
            additive_enabled=True,
            additive_reference="skeleton",
            root_motion_enabled=True,
            root_motion_joint="Root",
            translation_axes=("x", "z"),
            rotation_axes=("y",),
            root_motion_reference="animation",
            remove_from_pose=True,
            loop_correction=True,
        )
        self.assertTrue(result["loop"])
        self.assertEqual(
            result["root_motion"],
            {
                "enabled": True,
                "joint": "Root",
                "translation_axes": ["x", "z"],
                "rotation_axes": ["y"],
                "reference": "animation",
                "remove_from_pose": True,
                "loop_correction": True,
            },
        )

    def test_out_of_range_and_duplicate_markers_are_rejected(self):
        with self.assertRaisesRegex(metadata.MetadataError, "outside"):
            metadata.build_animation_metadata(
                [("event", 25)],
                frame_start=0,
                frame_end=24,
                frames_per_second=24,
                time_origin_frame=0,
            )
        with self.assertRaisesRegex(metadata.MetadataError, "duplicates"):
            metadata.build_animation_metadata(
                [("event", 12), ("event", 12)],
                frame_start=0,
                frame_end=24,
                frames_per_second=24,
                time_origin_frame=0,
            )

    def test_marker_names_match_cooker_validation(self):
        for invalid in ("", " padded", "padded ", "line\nbreak", "x" * 129):
            with self.subTest(invalid=invalid):
                with self.assertRaises(metadata.MetadataError):
                    metadata.build_animation_metadata(
                        [(invalid, 0)],
                        frame_start=0,
                        frame_end=1,
                        frames_per_second=24,
                        time_origin_frame=0,
                    )

    def test_invalid_animation_policy_is_rejected(self):
        arguments = {
            "markers": [],
            "frame_start": 0,
            "frame_end": 1,
            "frames_per_second": 24,
            "time_origin_frame": 0,
        }
        with self.assertRaisesRegex(metadata.MetadataError, "additive reference"):
            metadata.build_animation_metadata(
                **arguments,
                additive_reference="bind_pose",
            )
        with self.assertRaisesRegex(metadata.MetadataError, "duplicates"):
            metadata.build_animation_metadata(
                **arguments,
                root_motion_enabled=True,
                root_motion_joint="Root",
                translation_axes=("x", "x"),
            )
        with self.assertRaisesRegex(metadata.MetadataError, "joint"):
            metadata.build_animation_metadata(
                **arguments,
                root_motion_enabled=True,
                root_motion_joint="",
            )


class NodeMetadataTests(unittest.TestCase):
    def test_typed_node_identity_is_serialized(self):
        self.assertEqual(
            metadata.build_node_metadata("spawn_point", "base_north"),
            {
                "schema": 1,
                "node": {"kind": "spawn_point", "id": "base_north"},
            },
        )

    def test_node_id_is_optional(self):
        self.assertEqual(
            metadata.build_node_metadata("cover"),
            {"schema": 1, "node": {"kind": "cover"}},
        )

    def test_navigation_policy_uses_schema_one(self):
        self.assertEqual(
            metadata.build_node_metadata(
                "navigation_surface",
                "floor_main",
                navigation_role="surface",
                area_key="ground",
            ),
            {
                "schema": 1,
                "node": {
                    "kind": "navigation_surface",
                    "id": "floor_main",
                },
                "navigation": {
                    "role": "surface",
                    "area_key": "ground",
                },
            },
        )

    def test_static_acoustic_geometry_can_also_be_navigation(self):
        combined = metadata.build_node_metadata(
            "acoustic_geometry",
            "floor_main",
            navigation_role="surface",
            area_key="ground",
            acoustics_kind="geometry",
            geometry_class="static",
        )
        self.assertEqual(combined["navigation"]["role"], "surface")
        self.assertEqual(combined["acoustics"]["class"], "static")

    def test_navigation_requires_stable_id_and_complete_link_policy(self):
        with self.assertRaisesRegex(metadata.MetadataError, "id is required"):
            metadata.build_node_metadata(
                "navigation_surface",
                navigation_role="surface",
                area_key="ground",
            )
        with self.assertRaisesRegex(metadata.MetadataError, "radius"):
            metadata.build_node_metadata(
                "navigation_off_mesh_link",
                "jump_gap",
                navigation_role="off_mesh_link",
                area_key="jump",
                radius=0.0,
            )

    def test_invalid_node_kind_is_rejected(self):
        for invalid in ("", "SpawnPoint", "spawn-point", "spawn point", "_spawn"):
            with self.subTest(invalid=invalid):
                with self.assertRaises(metadata.MetadataError):
                    metadata.build_node_metadata(invalid)

    def test_merge_preserves_other_extras_and_rejects_owned_collision(self):
        owned = metadata.build_node_metadata("spawn_point")
        self.assertEqual(
            metadata.merge_extras({"vendor": {"enabled": True}}, owned),
            {
                "vendor": {"enabled": True},
                "blackflower": owned,
            },
        )
        with self.assertRaisesRegex(metadata.MetadataError, "already contain"):
                metadata.merge_extras({"blackflower": {}}, owned)

    def test_static_geometry_and_probe_volume_use_schema_one(self):
        self.assertEqual(
            metadata.build_node_metadata(
                "acoustic_geometry",
                "wall_north",
                acoustics_kind="geometry",
                geometry_class="static",
            )["acoustics"],
            {"kind": "geometry", "class": "static"},
        )
        probes = metadata.build_node_metadata(
            "acoustic_probe_volume",
            "ground_floor_probes",
            acoustics_kind="probe_volume",
            acoustic_zone="ground_floor",
        )
        self.assertEqual(probes["schema"], 1)
        self.assertEqual(
            probes["acoustics"],
            {"kind": "probe_volume", "zone": "ground_floor"},
        )
        self.assertNotIn("generation", probes["acoustics"])
        self.assertNotIn("spacing_meters", probes["acoustics"])

    def test_acoustic_nodes_require_stable_ids_and_matching_kinds(self):
        with self.assertRaisesRegex(metadata.MetadataError, "id is required"):
            metadata.build_node_metadata(
                "acoustic_zone",
                acoustics_kind="zone",
            )
        with self.assertRaisesRegex(metadata.MetadataError, "requires node kind"):
            metadata.build_node_metadata(
                "mesh",
                "wall",
                acoustics_kind="geometry",
            )


class MaterialMetadataTests(unittest.TestCase):
    def test_material_reference_is_portable(self):
        self.assertEqual(
            metadata.build_material_metadata("acoustics/materials/concrete"),
            {
                "schema": 1,
                "acoustics": {
                    "material": "acoustics/materials/concrete",
                },
            },
        )
        self.assertIsNone(metadata.build_material_metadata(""))

    def test_invalid_material_reference_is_rejected(self):
        for invalid in ("../concrete", "Acoustics/concrete", "a//b"):
            with self.subTest(invalid=invalid):
                with self.assertRaises(metadata.MetadataError):
                    metadata.build_material_metadata(invalid)


if __name__ == "__main__":
    unittest.main()
