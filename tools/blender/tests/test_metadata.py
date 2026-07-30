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


if __name__ == "__main__":
    unittest.main()
