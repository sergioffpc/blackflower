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
    def test_spawn_point_uses_map_schema_one(self):
        self.assertEqual(
            metadata.build_map_node_metadata(
                "spawn_point",
                "base_north",
                spawn_set="players",
                spawn_weight=2.0,
            ),
            {
                "schema": 1,
                "node": {"role": "spawn_point", "id": "base_north"},
                "spawn_point": {"set": "players", "weight": 2.0},
            },
        )

    def test_geometry_combines_domain_uses(self):
        combined = metadata.build_map_node_metadata(
            "geometry",
            "floor_main",
            render=True,
            collision=True,
            navigation="surface",
            acoustic_class="static",
        )
        self.assertEqual(combined["schema"], 1)
        self.assertEqual(
            combined["geometry"],
            {
                "render": True,
                "collision": True,
                "navigation": "surface",
                "acoustic_class": "static",
            },
        )

    def test_navigation_link_references_an_anchor(self):
        link = metadata.build_map_node_metadata(
            "navigation_link",
            "jump_gap",
            navigation_end="jump_end",
            navigation_area="jump",
            navigation_direction="one_way",
            navigation_radius=0.75,
        )
        self.assertEqual(
            link["navigation_link"],
            {
                "end": "jump_end",
                "area": "jump",
                "direction": "one_way",
                "radius": 0.75,
            },
        )

    def test_asset_backed_roles_require_portable_ids(self):
        self.assertEqual(
            metadata.build_map_node_metadata(
                "prefab_instance",
                "crate_one",
                asset="prefabs/crate",
            )["prefab_instance"],
            {"asset": "prefabs/crate"},
        )
        for invalid in ("../crate", "Prefabs/crate", "a//b"):
            with self.subTest(invalid=invalid):
                with self.assertRaises(metadata.MetadataError):
                    metadata.build_map_node_metadata(
                        "prefab_instance",
                        "crate_one",
                        asset=invalid,
                    )

    def test_node_role_and_id_are_closed_and_required(self):
        for role, identifier in (
            ("cover", "cover_one"),
            ("spawn_point", ""),
            ("spawn_point", "SpawnOne"),
        ):
            with self.subTest(role=role, identifier=identifier):
                with self.assertRaises(metadata.MetadataError):
                    metadata.build_map_node_metadata(role, identifier)

    def test_acoustic_zone_and_portal_payloads_are_typed(self):
        probes = metadata.build_map_node_metadata(
            "acoustic_zone",
            "ground_floor_probes",
            acoustic_zone_kind="probes",
            acoustic_zone="ground_floor",
        )
        portal = metadata.build_map_node_metadata(
            "acoustic_portal",
            "doorway",
            acoustic_zone_a="room_a_bounds",
            acoustic_zone_b="room_b_bounds",
            acoustic_controller="door_geometry",
            acoustic_initially_open=False,
        )
        self.assertEqual(
            probes["acoustic_zone"],
            {"kind": "probes", "zone": "ground_floor"},
        )
        self.assertEqual(
            portal["acoustic_portal"],
            {
                "zone_a": "room_a_bounds",
                "zone_b": "room_b_bounds",
                "controller": "door_geometry",
                "initially_open": False,
            },
        )
        with self.assertRaisesRegex(metadata.MetadataError, "must differ"):
            metadata.build_map_node_metadata(
                "acoustic_portal",
                "invalid",
                acoustic_zone_a="room_a_bounds",
                acoustic_zone_b="room_a_bounds",
            )

    def test_map_references_are_validated_together(self):
        nodes = [
            metadata.build_map_node_metadata(
                "navigation_anchor", "jump_end"
            ),
            metadata.build_map_node_metadata(
                "navigation_link",
                "jump_start",
                navigation_end="jump_end",
                navigation_area="jump",
            ),
            metadata.build_map_node_metadata(
                "acoustic_zone", "room", acoustic_zone_kind="identity"
            ),
            metadata.build_map_node_metadata(
                "acoustic_zone",
                "room_bounds",
                acoustic_zone_kind="bounds",
            ),
            metadata.build_map_node_metadata(
                "acoustic_zone",
                "room_probes",
                acoustic_zone_kind="probes",
                acoustic_zone="room",
            ),
        ]
        metadata.validate_map_references(nodes)

        with self.assertRaisesRegex(metadata.MetadataError, "missing node"):
            metadata.validate_map_references(
                [
                    metadata.build_map_node_metadata(
                        "navigation_link",
                        "jump_start",
                        navigation_end="missing",
                        navigation_area="jump",
                    )
                ]
            )
        with self.assertRaisesRegex(metadata.MetadataError, "duplicates"):
            metadata.validate_map_references([nodes[0], nodes[0]])

    def test_merge_preserves_other_extras_and_rejects_owned_collision(self):
        owned = metadata.build_map_node_metadata(
            "spawn_point", "base_north"
        )
        self.assertEqual(
            metadata.merge_extras({"vendor": {"enabled": True}}, owned),
            {
                "vendor": {"enabled": True},
                "blackflower": owned,
            },
        )
        with self.assertRaisesRegex(metadata.MetadataError, "already contain"):
            metadata.merge_extras({"blackflower": {}}, owned)


class MaterialMetadataTests(unittest.TestCase):
    def test_complete_surface_mapping_uses_schema_one(self):
        self.assertEqual(
            metadata.build_map_material_metadata(
                physics_material="materials/physics/concrete",
                navigation_area="ground",
                acoustic_material="acoustics/materials/concrete",
            ),
            {
                "schema": 1,
                "material": {
                    "physics_material": "materials/physics/concrete",
                    "navigation_area": "ground",
                    "acoustic_material": "acoustics/materials/concrete",
                },
            },
        )
        self.assertIsNone(metadata.build_map_material_metadata())

    def test_invalid_surface_mapping_is_rejected(self):
        for invalid in ("../concrete", "Acoustics/concrete", "a//b"):
            with self.subTest(invalid=invalid):
                with self.assertRaises(metadata.MetadataError):
                    metadata.build_map_material_metadata(
                        acoustic_material=invalid
                    )
        with self.assertRaises(metadata.MetadataError):
            metadata.build_map_material_metadata(navigation_area="Ground")


if __name__ == "__main__":
    unittest.main()
