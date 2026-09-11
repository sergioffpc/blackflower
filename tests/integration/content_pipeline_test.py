"""Production-to-consumption checks through the CLI and runtime harness."""

from collections.abc import Callable
import hashlib
import json
import os
import pathlib
import shutil
import struct
import subprocess
import sys
import tempfile
import unittest
from typing import Any

from cryptography.hazmat.primitives.asymmetric import ed25519
from pxr import Sdf
from pxr import Usd

from cooker import pack
from cooker import pipeline
from cooker import scene

ROOT = pathlib.Path(__file__).resolve().parents[2]
HARNESS = pathlib.Path(
    os.environ.get(
        "BLACKFLOWER_CONTENT_HARNESS",
        ROOT / "build/debug/blackflower_content_harness",
    )
)


def _signer_failing_on_third_pack(
    key: ed25519.Ed25519PrivateKey,
) -> Callable[[bytes], bytes]:
    calls = 0

    def sign(data: bytes) -> bytes:
        nonlocal calls
        calls += 1
        if calls == 3:
            raise RuntimeError("signing failed")
        return key.sign(data)

    return sign


def _harness_command(
    pack_path: pathlib.Path, public: pathlib.Path
) -> list[str]:
    def runtime_path(path: pathlib.Path) -> str:
        if HARNESS.suffix == ".exe" and sys.platform == "linux":
            return subprocess.check_output(
                ["wslpath", "-w", str(path)], text=True
            ).strip()
        return str(path)

    return [
        str(HARNESS),
        runtime_path(pack_path),
        runtime_path(public),
    ]


class ContentPipelineTest(unittest.TestCase):

    def test_cooks_role_specific_scene_descriptions(self):
        with tempfile.TemporaryDirectory() as directory:
            work = pathlib.Path(directory)
            authored = work / "authored"
            shutil.copytree(ROOT / "tests/integration/fixtures", authored)
            private, public = work / "private.pem", work / "public.key"
            self._generate_signing_keys(private, public)
            output = work / "cooked"
            identity = self._cook(
                authored / "scenes/roles.usda",
                output,
                private,
            )["content_build_id"]
            shutil.rmtree(authored)
            self.assertNotEqual(identity, bytes(32).hex())
            self.assertEqual(
                {path.name for path in output.iterdir()},
                {"roles.bfserver", "roles.bfagent", "roles.bfclient"},
            )
            contents = {
                role: self._consume(output / f"roles.bf{role}", public)
                for role in ("server", "agent", "client")
            }
            self._check_role_scenes(contents, identity)

    def _check_role_scenes(
        self, contents: dict[str, dict[str, Any]], identity: str
    ) -> None:
        for role, content in contents.items():
            self.assertEqual(content["scene_type"], role)
            self.assertEqual(content["content_build_id"], identity)
        self._check_headless_role_scenes(contents)
        self._check_client_role_scene(contents["client"]["entities"])

    def _check_headless_role_scenes(
        self, contents: dict[str, dict[str, Any]]
    ) -> None:
        static_collision_ids = ["collision-only", "mixed"]
        for role in ("server", "agent"):
            entities = contents[role]["entities"]
            expected_ids = (
                ["collision-only", "dynamic", "mixed"]
                if role == "server"
                else static_collision_ids
            )
            self.assertEqual(
                [entity["id"] for entity in entities], expected_ids
            )
            self.assertTrue(all(entity["colliders"] for entity in entities))
            self.assertTrue(
                all(
                    entity["collider_asset_id"] is not None
                    for entity in entities
                )
            )
            self.assertTrue(
                all(
                    entity["visual_ref"] is None and entity["audio_ref"] is None
                    for entity in entities
                )
            )
        server_by_id = {
            entity["id"]: entity for entity in contents["server"]["entities"]
        }
        self.assertEqual(
            server_by_id["dynamic"]["collision_domain"],
            "authoritative_dynamic",
        )
        self.assertTrue(
            all(
                server_by_id[identity]["collision_domain"] == "session_static"
                for identity in static_collision_ids
            )
        )

    def _check_client_role_scene(self, client: list[dict[str, Any]]) -> None:
        self.assertEqual(
            [entity["id"] for entity in client],
            [
                "audio-only",
                "collision-only",
                "dynamic",
                "mixed",
                "visual-only",
            ],
        )
        by_id = {entity["id"]: entity for entity in client}
        self.assertEqual(by_id["audio-only"]["audio_ref"], "audio.ambient")
        self.assertEqual(by_id["visual-only"]["visual_ref"], "visual.target")
        self.assertEqual(by_id["mixed"]["visual_ref"], "visual.crate")
        self.assertEqual(by_id["mixed"]["audio_ref"], "audio.crate")
        self.assertEqual(by_id["dynamic"]["visual_ref"], "visual.mover")
        self.assertIsNone(by_id["dynamic"]["collider_asset_id"])
        self.assertIsNone(by_id["audio-only"]["collider_asset_id"])
        self.assertIsNone(by_id["visual-only"]["collider_asset_id"])
        self.assertEqual(
            [entity["id"] for entity in client if entity["colliders"]],
            ["collision-only", "mixed"],
        )

    def test_loader_rejects_an_authenticated_unexpected_role(self):
        with tempfile.TemporaryDirectory() as directory:
            work = pathlib.Path(directory)
            private, public = work / "private.pem", work / "public.key"
            self._generate_signing_keys(private, public)
            output = work / "cooked"
            self._cook(
                ROOT / "tests/integration/fixtures/scenes/roles.usda",
                output,
                private,
            )
            result = subprocess.run(
                _harness_command(output / "roles.bfclient", public)
                + ["server"],
                capture_output=True,
                text=True,
                check=False,
            )
            self.assertEqual(result.returncode, 1)
            self.assertEqual(result.stdout, "")
            self.assertEqual(
                result.stderr, "content rejected: unexpected pack role\n"
            )

    def test_failed_role_pack_preparation_publishes_nothing(self):
        with tempfile.TemporaryDirectory() as directory:
            work = pathlib.Path(directory)
            output = work / "cooked"
            key = ed25519.Ed25519PrivateKey.generate()

            with self.assertRaisesRegex(RuntimeError, "signing failed"):
                pipeline.cook(
                    ROOT / "tests/integration/fixtures/scenes/roles.usda",
                    output,
                    key.public_key().public_bytes_raw(),
                    _signer_failing_on_third_pack(key),
                )
            self.assertFalse(output.exists())
            self.assertFalse(output.with_name("cooked.lock").exists())
            self.assertEqual(list(work.iterdir()), [])

    def test_failed_generation_preserves_previous_complete_generation(self):
        with tempfile.TemporaryDirectory() as directory:
            work = pathlib.Path(directory)
            previous_output = work / "published"
            failed_output = work / "replacement"
            key = ed25519.Ed25519PrivateKey.generate()
            public = key.public_key().public_bytes_raw()
            source = ROOT / "tests/integration/fixtures/scenes/roles.usda"
            pipeline.cook(source, previous_output, public, key.sign)
            previous = {
                path.name: path.read_bytes()
                for path in previous_output.iterdir()
            }
            with self.assertRaisesRegex(RuntimeError, "signing failed"):
                pipeline.cook(
                    source,
                    failed_output,
                    public,
                    _signer_failing_on_third_pack(key),
                )

            self.assertEqual(
                {
                    path.name: path.read_bytes()
                    for path in previous_output.iterdir()
                },
                previous,
            )
            self.assertFalse(failed_output.exists())
            self.assertFalse(
                failed_output.with_name("replacement.lock").exists()
            )

    def test_scene_entity_description_matches_reference_encoding(self):
        reference = json.loads(
            (ROOT / "tests/fixtures/packs/reference.json").read_text()
        )
        description: scene.SceneEntityDescription = {
            "id": "reference",
            "position_m": [7, 0, 0],
            "rotation_xyzw": [0, 0, 0, 1],
            "scale": 1,
            "collision": {
                "domain": scene.CollisionDomain.SESSION_STATIC,
                "colliders": [
                    {
                        "center_m": [-3, 1, 0],
                        "dimensions_m": [2, 2, 2],
                        "rotation_xyzw": [0, 0, 0, 1],
                    },
                    {
                        "center_m": [1, 5, 2],
                        "dimensions_m": [1, 1, 1],
                        "rotation_xyzw": [0, 0, 0, 1],
                    },
                ],
            },
            "visual_ref": None,
            "audio_ref": None,
        }
        encoded = scene.encode(
            {"entities": [description]}, scene.SceneRole.SERVER
        )
        self.assertEqual(encoded.hex(), reference["scenes"]["server"])

    def test_entity_dependencies_are_relocatable_and_reproducible(self):
        with tempfile.TemporaryDirectory() as directory:
            work = pathlib.Path(directory)
            authored = work / "authored"
            shutil.copytree(ROOT / "tests/integration/fixtures", authored)
            private, public = work / "private.pem", work / "public.key"
            self._generate_signing_keys(private, public)
            original = self._cook(
                authored / "scenes/entities.usda", work / "first", private
            )
            relocated = work / "relocated"
            shutil.copytree(authored, relocated)
            repeated = self._cook(
                relocated / "scenes/entities.usda", work / "repeated", private
            )
            self.assertEqual(original, repeated)
            for name in ("server", "agent", "client"):
                filename = f"entities.bf{name}"
                self.assertEqual(
                    (work / "first" / filename).read_bytes(),
                    (work / "repeated" / filename).read_bytes(),
                )
            definition = relocated / "entities/box.usda"
            definition.write_text(
                definition.read_text().replace(
                    "double size = 1", "double size = 2"
                )
            )
            changed = self._cook(
                relocated / "scenes/entities.usda", work / "changed", private
            )
            self.assertNotEqual(original, changed)

    def test_entity_identity_survives_rename_and_definition_reuse(self):
        with tempfile.TemporaryDirectory() as directory:
            work = pathlib.Path(directory)
            shutil.copytree(
                ROOT / "tests/integration/fixtures", work / "inputs"
            )
            source = work / "inputs/scenes/entities.usda"
            stage = Usd.Stage.Open(str(source))
            layer = stage.GetRootLayer()
            edits = Sdf.BatchNamespaceEdit()
            edits.Add("/Scene/Entities/Box", "/Scene/Entities/Renamed")
            self.assertTrue(layer.Apply(edits))
            copy = stage.DefinePrim("/Scene/Entities/Copy", "Xform")
            copy.GetReferences().AddReference("../entities/box.usda")
            copy.CreateAttribute(
                "blackflower:id", Sdf.ValueTypeNames.String
            ).Set("box-02")
            layer.Save()
            private, public = work / "private.pem", work / "public.key"
            self._generate_signing_keys(private, public)
            self._cook(source, work / "cooked", private)
            loaded = subprocess.run(
                _harness_command(work / "cooked/entities.bfagent", public),
                capture_output=True,
                text=True,
                check=False,
            )
            self.assertEqual(loaded.returncode, 0, loaded.stderr)
            data = json.loads(loaded.stdout)
            self.assertEqual(
                [e["id"] for e in data["entities"]],
                ["box-01", "box-02", "floor-main"],
            )
            self.assertEqual(
                data["entities"][0]["collider_asset_id"],
                data["entities"][1]["collider_asset_id"],
            )
            self.assertEqual(
                [len(e["colliders"]) for e in data["entities"]], [2, 2, 1]
            )

    def test_referenced_entities_preserve_owned_oriented_collision(self):
        with tempfile.TemporaryDirectory() as directory:
            work = pathlib.Path(directory)
            authored = work / "authored"
            shutil.copytree(ROOT / "tests/integration/fixtures", authored)
            private, public = work / "private.pem", work / "public.key"
            self._generate_signing_keys(private, public)
            source = authored / "scenes/entities.usda"
            output = work / "cooked"
            identities = self._cook(source, output, private)
            shutil.rmtree(authored)
            for name in ("server", "agent", "client"):
                isolated = work / f"{name}-cenário.pack"
                shutil.copyfile(output / f"entities.bf{name}", isolated)
                loaded = subprocess.run(
                    _harness_command(isolated, public),
                    capture_output=True,
                    text=True,
                    check=False,
                )
                self.assertEqual(loaded.returncode, 0, loaded.stderr)
                content = json.loads(loaded.stdout)
                self.assertEqual(
                    content["content_build_id"], identities["content_build_id"]
                )
                self.assertEqual(
                    [e["id"] for e in content["entities"]],
                    ["box-01", "floor-main"],
                )
                self._check_entity_boxes(content)

    def _check_entity_boxes(self, content: dict[str, Any]) -> None:
        box_entity, floor_entity = content["entities"]
        self.assertEqual(box_entity["position_m"], [2, 1, 3])
        self.assertEqual(box_entity["scale"], 2)
        body, cap = box_entity["colliders"]
        (floor,) = floor_entity["colliders"]
        for actual, expected in zip(cap["center_m"], (1.0, 0.0, 0.0)):
            self.assertAlmostEqual(actual, expected, places=5)
        self.assertEqual(body["dimensions_m"], [1, 1, 1])
        self.assertEqual(cap["dimensions_m"], [0.5, 0.5, 0.5])
        for actual, expected in zip(floor["dimensions_m"], (20.0, 0.2, 20.0)):
            self.assertAlmostEqual(actual, expected, places=5)
        for actual, expected in zip(floor["center_m"], (0.0, -0.1, 0.0)):
            self.assertAlmostEqual(actual, expected, places=5)
        for actual, expected in zip(
            body["rotation_xyzw"],
            (0, 0, 0, 1),
        ):
            self.assertAlmostEqual(actual, expected, places=5)

    def test_entities_and_bounds_are_optional(self):
        with tempfile.TemporaryDirectory() as directory:
            work = pathlib.Path(directory)
            shutil.copytree(
                ROOT / "tests/integration/fixtures", work / "inputs"
            )
            private, public = work / "private.pem", work / "public.key"
            self._generate_signing_keys(private, public)
            definition = work / "inputs/entities/box.usda"
            stage = Usd.Stage.Open(str(definition))
            stage.RemovePrim("/Entity/Bounds")
            stage.GetRootLayer().Save()
            source = work / "inputs/scenes/entities.usda"
            for empty in (False, True):
                if empty:
                    stage = Usd.Stage.Open(str(source))
                    stage.RemovePrim("/Scene/Entities")
                    stage.GetRootLayer().Save()
                self._cook(source, work / str(empty), private)
                for role in ("server", "agent", "client"):
                    content = self._consume(
                        work / str(empty) / f"entities.bf{role}", public
                    )
                    self.assertEqual(
                        set(content),
                        {
                            "scene_type",
                            "content_build_id",
                            "scene_asset_id",
                            "entities",
                        },
                    )
                    if empty:
                        self.assertEqual(content["entities"], [])
                    else:
                        self.assertEqual(
                            [entity["id"] for entity in content["entities"]],
                            ["floor-main"],
                        )
                        self.assertTrue(content["entities"][0]["colliders"])

    def test_world_owns_one_scene_and_unloads_it(self):
        with tempfile.TemporaryDirectory() as directory:
            work = pathlib.Path(directory)
            private, public = work / "private.pem", work / "public.key"
            self._generate_signing_keys(private, public)
            self._cook(
                ROOT / "tests/integration/fixtures/scenes/entities.usda",
                work / "cooked",
                private,
            )
            for role in ("server", "agent", "client"):
                loaded = subprocess.run(
                    _harness_command(
                        work / "cooked" / f"entities.bf{role}", public
                    )
                    + ["scene"],
                    capture_output=True,
                    text=True,
                    check=False,
                )
                self.assertEqual(loaded.returncode, 0, loaded.stderr)
                result = json.loads(loaded.stdout)
                self.assertTrue(result["lifecycle_verified"])
                for actual, expected in zip(result["cap_center"], (2, 1, 1)):
                    self.assertAlmostEqual(actual, expected, places=5)
                self.assertEqual(result["body_dimensions"], [2, 2, 2])

    def test_independently_encoded_reference_pack(self):
        reference = json.loads(
            (ROOT / "tests/fixtures/packs/reference.json").read_text()
        )
        with tempfile.TemporaryDirectory() as directory:
            work = pathlib.Path(directory)
            public = bytes.fromhex(reference["public_key"])
            (work / "public.key").write_bytes(public)
            for name in ("server", "agent", "client"):
                self._check_reference(work, public, reference, name)

    def _check_reference(
        self,
        work: pathlib.Path,
        public: bytes,
        reference: dict[str, Any],
        name: str,
    ) -> None:
        data = (ROOT / f"tests/fixtures/packs/reference.bf{name}").read_bytes()
        verified = pack.verify(data, [public])
        self.assertEqual(verified.pack_type, pack.PackType[name.upper()])
        self.assertEqual(verified.payload.hex(), reference["scenes"][name])
        self.assertEqual(
            verified.content_build_id.hex(), reference["content_build_id"]
        )
        (work / f"reference.bf{name}").write_bytes(data)
        result = subprocess.run(
            _harness_command(work / f"reference.bf{name}", work / "public.key"),
            capture_output=True,
            text=True,
            check=False,
        )
        self.assertEqual(result.returncode, 0, result.stderr)
        content = json.loads(result.stdout)
        self.assertEqual(content["scene_type"], name)
        self.assertEqual(content["scene_asset_id"], reference["scene_asset_id"])
        self.assertEqual(
            content["entities"][0]["collider_asset_id"],
            reference["collider_asset_id"],
        )
        self.assertEqual(
            content["scene_asset_id"],
            hashlib.sha256(
                b"Blackflower.Scene.v1" + verified.payload
            ).hexdigest(),
        )
        self.assertEqual(
            content["content_build_id"], reference["content_build_id"]
        )

    def _consume(
        self, path: pathlib.Path, public: pathlib.Path
    ) -> dict[str, Any]:
        result = subprocess.run(
            _harness_command(path, public),
            capture_output=True,
            text=True,
            check=False,
        )
        self.assertEqual(result.returncode, 0, result.stderr)
        return json.loads(result.stdout)

    def _generate_signing_keys(
        self, private: pathlib.Path, public: pathlib.Path
    ) -> None:
        result = subprocess.run(
            [
                sys.executable,
                "-m",
                "cooker",
                "keygen",
                "--private-key",
                str(private),
                "--public-key",
                str(public),
            ],
            capture_output=True,
            text=True,
            check=False,
        )
        self.assertEqual(result.returncode, 0, result.stderr)
        self.assertEqual(private.stat().st_mode & 0o777, 0o600)
        self.assertEqual(len(public.read_bytes()), 32)
        self.assertEqual(
            json.loads(result.stdout),
            {"privateKey": str(private), "publicKey": str(public)},
        )

    def _cook(
        self, source: pathlib.Path, output: pathlib.Path, private: pathlib.Path
    ) -> dict[str, str]:
        result = subprocess.run(
            [
                sys.executable,
                "-m",
                "cooker",
                "cook",
                "--source",
                str(source),
                "--output",
                str(output),
                "--private-key",
                str(private),
            ],
            capture_output=True,
            text=True,
            check=False,
        )
        self.assertEqual(result.returncode, 0, result.stderr)
        return json.loads(result.stdout)


class InvalidPacksTest(unittest.TestCase):
    """Rejects malformed artifacts through the actual C++ file loader."""

    def setUp(self) -> None:
        self.directory = tempfile.TemporaryDirectory()
        self.addCleanup(self.directory.cleanup)
        self.work = pathlib.Path(self.directory.name)
        self.key = ed25519.Ed25519PrivateKey.generate()
        self.public = self.work / "trusted.pub"
        self.public.write_bytes(self.key.public_key().public_bytes_raw())
        # Independent encoding from schemas/pack/v1.md, without cooker helpers.
        self.provenance = bytes(64) + 5 * (b"\x01\x00\x00\x00v")
        self.record_start = 104 + 4 + len(self.provenance)
        self.payload_start = self.record_start + 64

    def _artifact(
        self, payload: bytes = bytes(4), magic: bytes = b"BFAGNT1\0"
    ) -> bytearray:
        record = struct.pack(
            "<4I2Q32s",
            1,
            1,
            1,
            0,
            0,
            len(payload),
            hashlib.sha256(payload).digest(),
        )
        manifest = (
            struct.pack("<I", len(self.provenance)) + self.provenance + record
        )
        header = struct.pack(
            "<8s2I3Q32s32s",
            magic,
            1,
            1,
            104 + len(manifest) + len(payload) + 64,
            len(manifest),
            len(payload),
            hashlib.sha256(self.public.read_bytes()).digest(),
            bytes(32),
        )
        return self._sign(bytearray(header + manifest + payload + bytes(64)))

    def _sign(
        self, data: bytearray, payload_start: int | None = None
    ) -> bytearray:
        if payload_start is None:
            payload_start = self.payload_start
        transcript = b"Blackflower.Pack.v1\0" + data[:payload_start]
        data[-64:] = self.key.sign(transcript)
        # Independently establish valid signatures on malformed fixtures.
        self.key.public_key().verify(bytes(data[-64:]), transcript)
        return data

    def _reject(self, data: bytes | bytearray, diagnostic: str) -> None:
        path = self.work / "invalid.pack"
        path.write_bytes(data)
        result = subprocess.run(
            _harness_command(path, self.public),
            capture_output=True,
            text=True,
            check=False,
            timeout=10,
        )
        self.assertEqual(result.returncode, 1, result.stderr)
        self.assertEqual(result.stdout, "", "rejected content was published")
        self.assertEqual(result.stderr, f"content rejected: {diagnostic}\n")

    def _accept(self, data: bytearray, scene_type: str) -> None:
        path = self.work / "control.pack"
        path.write_bytes(data)
        result = subprocess.run(
            _harness_command(path, self.public),
            capture_output=True,
            text=True,
            check=False,
            timeout=10,
        )
        self.assertEqual(result.returncode, 0, result.stderr)
        self.assertEqual(json.loads(result.stdout)["scene_type"], scene_type)

    def test_independent_signed_control_is_accepted(self) -> None:
        self._accept(self._artifact(), "agent")

    def test_rejects_tampering_without_publishing_content(self) -> None:
        for offset, diagnostic in (
            (72, "invalid pack signature"),
            (108, "invalid pack signature"),
            (self.record_start + 32, "invalid pack signature"),
            (self.payload_start, "resource digest mismatch"),
            (-1, "invalid pack signature"),
        ):
            with self.subTest(offset=offset):
                data = self._artifact()
                data[offset] ^= 1
                self._reject(data, diagnostic)

    def test_pack_cannot_supply_its_own_trust(self) -> None:
        # Even a signed payload carrying the attacker's public key grants no
        # authority when the independently provisioned trust is different.
        data = self._artifact(self.key.public_key().public_bytes_raw())
        self.public.write_bytes(
            ed25519.Ed25519PrivateKey.generate().public_key().public_bytes_raw()
        )
        self._reject(data, "unknown signing key")

    def test_missing_and_truncated_files(self) -> None:
        missing = self.work / "missing.pack"
        result = subprocess.run(
            _harness_command(missing, self.public),
            capture_output=True,
            text=True,
            check=False,
            timeout=10,
        )
        self.assertEqual(result.returncode, 1)
        self.assertEqual(result.stdout, "")
        self.assertEqual(
            result.stderr, "content rejected: pack mapping failed\n"
        )
        data = self._artifact()
        for size in (1, 103, 104, 167, self.payload_start, len(data) - 1):
            with self.subTest(size=size):
                self._reject(
                    data[:size],
                    "invalid pack length"
                    if size < 168
                    else "invalid pack layout",
                )

    def test_signed_header_rejects_unsupported_and_overflowing_layouts(
        self,
    ) -> None:
        for offset, encoding, value, diagnostic in (
            (0, "8s", b"UNKNOWN\0", "unsupported pack format"),
            (8, "I", 2, "unsupported pack format"),
            (12, "I", 0, "unsupported resource count"),
            (12, "I", 2, "unsupported resource count"),
            (12, "I", 2**32 - 1, "unsupported resource count"),
            (16, "Q", 2**64 - 1, "invalid pack layout"),
            (24, "Q", 0, "invalid pack layout"),
            (24, "Q", 2**64 - 1, "invalid pack layout"),
            (32, "Q", 2**64 - 1, "invalid pack layout"),
        ):
            with self.subTest(offset=offset, value=value):
                data = self._artifact()
                struct.pack_into("<" + encoding, data, offset, value)
                self._reject(self._sign(data), diagnostic)
        self._reject(self._artifact() + b"extra", "invalid pack layout")

    def test_signed_provenance_requires_complete_printable_fields(self) -> None:
        for offset, value in ((104, 2**32 - 1), (172, 0), (172, 2**32 - 1)):
            with self.subTest(offset=offset, value=value):
                data = self._artifact()
                struct.pack_into("<I", data, offset, value)
                self._reject(self._sign(data), "invalid provenance layout")
        for value in (0, 31, 127, 255):
            with self.subTest(character=value):
                data = self._artifact()
                data[176] = value
                self._reject(self._sign(data), "invalid provenance layout")

    def test_signed_resource_rejects_identity_schema_and_range_errors(
        self,
    ) -> None:
        for offset, encoding, value in (
            (0, "I", 0),
            (0, "I", 2),
            (4, "I", 0),
            (4, "I", 2**32 - 1),
            (8, "I", 2),
            (12, "I", 1),
            (16, "Q", 1),
            (16, "Q", 2**64 - 1),
            (24, "Q", 0),
            (24, "Q", 11),
            (24, "Q", 13),
            (24, "Q", 2**64 - 1),
        ):
            with self.subTest(offset=offset, value=value):
                data = self._artifact()
                struct.pack_into(
                    "<" + encoding, data, self.record_start + offset, value
                )
                self._reject(
                    self._sign(data),
                    "invalid resource identity, schema or range",
                )
        data = self._artifact()
        data[self.record_start + 32] ^= 1
        self._reject(self._sign(data), "resource digest mismatch")

    def test_duplicate_and_overlapping_entries_are_not_supported(self) -> None:
        data = self._artifact()
        record = data[self.record_start : self.payload_start]
        data[self.payload_start : self.payload_start] = record
        struct.pack_into("<I", data, 12, 2)
        struct.pack_into("<Q", data, 16, len(data))
        struct.pack_into("<Q", data, 24, self.payload_start - 104 + 64)
        self._reject(
            self._sign(data, self.payload_start + 64),
            "unsupported resource count",
        )

    def test_signed_scene_rejects_missing_records_and_unknown_kinds(
        self,
    ) -> None:
        entity = (
            struct.pack("<I", 1)
            + b"a"
            + struct.pack("<8fI", 0, 0, 0, 0, 0, 0, 1, 1, 1)
            + struct.pack("<2I", 1, 1)
        )
        for payload, diagnostic in (
            (b"", "invalid scene length"),
            (bytes(3), "invalid scene length"),
            (bytes(5), "invalid scene length"),
            (struct.pack("<I", 2**32 - 1), "invalid scene length"),
            (
                struct.pack("<I", 1) + entity + struct.pack("<I", 257),
                "unsupported collider kind",
            ),
        ):
            with self.subTest(payload=payload):
                self._reject(self._artifact(payload), diagnostic)

    def test_signed_records_cannot_be_truncated(self) -> None:
        entity = (
            struct.pack("<I", 1)
            + b"a"
            + struct.pack("<8fI", 0, 0, 0, 0, 0, 0, 1, 1, 1)
            + struct.pack("<2I", 1, 1)
        )
        bound = struct.pack("<I10f", 1, 0, 0, 0, 1, 1, 1, 0, 0, 0, 1)
        record = entity + bound
        header = struct.pack("<I", 1)
        self._accept(self._artifact(header + record), "agent")
        for size in range(len(record)):
            with self.subTest(size=size):
                self._reject(
                    self._artifact(header + record[:size]),
                    "invalid scene length",
                )


if __name__ == "__main__":
    unittest.main()
