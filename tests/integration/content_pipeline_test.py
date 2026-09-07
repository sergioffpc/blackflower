"""Production-to-consumption checks through the CLI and runtime harness."""

import hashlib
import json
import os
import pathlib
import struct
import subprocess
import sys
import tempfile
import unittest
from unittest import mock

from cryptography import exceptions
from cryptography.hazmat.primitives.asymmetric import ed25519
from cryptography.hazmat.primitives import serialization
from pxr import Usd

from blackflower_cooker import pack
from blackflower_cooker import pipeline

ROOT = pathlib.Path(__file__).resolve().parents[2]
HARNESS = pathlib.Path(
    os.environ.get(
        "BLACKFLOWER_CONTENT_HARNESS",
        ROOT / "build/debug/blackflower_content_harness",
    )
)


def _harness_command(pack_path, role, profile, public):
    def runtime_path(path):
        if HARNESS.suffix == ".exe" and sys.platform == "linux":
            return subprocess.check_output(
                ["wslpath", "-w", str(path)], text=True
            ).strip()
        return str(path)

    return [
        str(HARNESS),
        runtime_path(pack_path),
        role,
        profile,
        runtime_path(public),
    ]


class ContentPipelineTest(unittest.TestCase):

    def test_independently_encoded_reference_packs(self):
        reference = json.loads(
            (ROOT / "tests/fixtures/packs/reference.json").read_text()
        )
        with tempfile.TemporaryDirectory() as directory:
            work = pathlib.Path(directory)
            public = bytes.fromhex(reference["public_key"])
            (work / "public.key").write_bytes(public)
            for role, value in [("client", 1), ("server", 2)]:
                data = (
                    ROOT / f"tests/fixtures/packs/reference.bf{role}"
                ).read_bytes()
                verified = pack.verify(data, value, value, [public])
                self.assertEqual(verified.payload.hex(), reference["scene"])
                self.assertEqual(
                    verified.pack_id.hex(), reference[f"{role}_id"]
                )
                self.assertEqual(verified.build_id.hex(), reference["build_id"])
                (work / f"reference.bf{role}").write_bytes(data)
                result = subprocess.run(
                    _harness_command(
                        work / f"reference.bf{role}",
                        role,
                        "windows" if value == 1 else "linux",
                        work / "public.key",
                    ),
                    capture_output=True,
                    text=True,
                    check=False,
                )
                self.assertEqual(result.returncode, 0, result.stderr)
                self.assertEqual(
                    json.loads(result.stdout)["pack_id"],
                    reference[f"{role}_id"],
                )

    def test_each_role_reads_the_fixed_scene_without_its_counterpart(self):
        with tempfile.TemporaryDirectory() as directory:
            work = pathlib.Path(directory)
            key = ed25519.Ed25519PrivateKey.generate()
            private = work / "private.pem"
            private.write_bytes(
                key.private_bytes(
                    serialization.Encoding.PEM,
                    serialization.PrivateFormat.PKCS8,
                    serialization.NoEncryption(),
                )
            )
            public = work / "public.key"
            public.write_bytes(
                key.public_key().public_bytes(
                    serialization.Encoding.Raw, serialization.PublicFormat.Raw
                )
            )
            output = work / "pair"
            result = subprocess.run(
                [
                    sys.executable,
                    "-m",
                    "blackflower_cooker",
                    "cook",
                    "--source",
                    str(ROOT / "assets/scenes/mvp.usda"),
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
            self.assertEqual(
                sorted(p.name for p in output.iterdir()),
                ["mvp.bfclient", "mvp.bfserver"],
            )
            results = []
            for role, profile in [("client", "windows"), ("server", "linux")]:
                isolated = work / role
                isolated.mkdir()
                pack_path = isolated / f"scenario.bf{role}"
                pack_path.write_bytes((output / f"mvp.bf{role}").read_bytes())
                loaded = subprocess.run(
                    _harness_command(pack_path, role, profile, public),
                    capture_output=True,
                    text=True,
                    check=False,
                    cwd=isolated,
                )
                self.assertEqual(loaded.returncode, 0, loaded.stderr)
                content = json.loads(loaded.stdout)
                self.assertEqual(content["interior_mm"], [20000, 20000])
                self.assertEqual(content["capsule_mm"], [1800, 600])
                self.assertEqual(content["box_count"], 2)
                self.assertEqual(
                    content["spawns_mm"],
                    [
                        [-8000, 0, -8000],
                        [-8000, 0, 8000],
                        [8000, 0, -8000],
                        [8000, 0, 8000],
                    ],
                )
                results.append(content)
            self.assertNotEqual(results[0]["pack_id"], results[1]["pack_id"])
            self.assertEqual(
                results[0]["scenario_build_id"], results[1]["scenario_build_id"]
            )


class ContentFailureTest(unittest.TestCase):

    def setUp(self):
        self.temporary = tempfile.TemporaryDirectory()
        self.addCleanup(self.temporary.cleanup)
        self.work = pathlib.Path(self.temporary.name)
        self.key = ed25519.Ed25519PrivateKey.generate()
        self.public = self.key.public_key().public_bytes(
            serialization.Encoding.Raw, serialization.PublicFormat.Raw
        )
        self.public_path = self.work / "public.key"
        self.public_path.write_bytes(self.public)
        self.source = self.work / "source.usda"
        self.source.write_bytes((ROOT / "assets/scenes/mvp.usda").read_bytes())
        self.output = self.work / "pair"

    def _cook(self, signer=None):
        return pipeline.cook_pair(
            self.source, self.output, self.public, signer or self.key.sign
        )

    def _assert_rejected(
        self, data, role="client", profile="windows", public=None
    ):
        keys = [public or self.public]
        with self.assertRaises((ValueError, exceptions.InvalidSignature)):
            pack.verify(
                data,
                1 if role == "client" else 2,
                1 if profile == "windows" else 2,
                keys,
            )
        candidate = self.work / "candidate.bfclient"
        candidate.write_bytes(data)
        self.public_path.write_bytes(keys[0])
        result = subprocess.run(
            _harness_command(candidate, role, profile, self.public_path),
            capture_output=True,
            text=True,
            check=False,
        )
        self.assertNotEqual(result.returncode, 0)
        self.assertIn("content rejected:", result.stderr)
        self.assertEqual(result.stdout, "")

    def test_wrong_role_profile_unknown_key_and_tampering(self):
        self._cook()
        original = (self.output / "mvp.bfclient").read_bytes()
        self._assert_rejected(original, role="server", profile="linux")
        self._assert_rejected(original, profile="linux")
        other_key = (
            ed25519.Ed25519PrivateKey.generate()
            .public_key()
            .public_bytes(
                serialization.Encoding.Raw, serialization.PublicFormat.Raw
            )
        )
        self._assert_rejected(original, public=other_key)
        for offset in (12, 48, 80, 120, len(original) - 65, len(original) - 1):
            with self.subTest(offset=offset):
                changed = bytearray(original)
                changed[offset] ^= 1
                self._assert_rejected(bytes(changed))
        for size in (0, 8, 111, 175, len(original) - 1):
            with self.subTest(size=size):
                self._assert_rejected(original[:size])

    def test_valid_signatures_do_not_bypass_structure_or_scene_validation(self):
        self._cook()
        original = (self.output / "mvp.bfclient").read_bytes()
        manifest_size = int.from_bytes(original[32:40], "little")
        end = 112 + manifest_size
        provenance_size = int.from_bytes(original[112:116], "little")
        entry = 116 + provenance_size
        mutations = [
            (8, "<I", 2),
            (20, "<I", 2),
            (24, "<Q", len(original) + 1),
            (entry, "<I", 0),
            (entry + 8, "<I", 99),
            (entry + 16, "<Q", 1),
            (entry + 24, "<Q", 153),
            (end + 16, "<I", 1700),
            (end + 24, "<I", 3),
            (end + 88 + 4, "<i", -10000),
            (end + 88 + 8, "<i", 1),
            (end + 104, "<I", 1),
        ]
        for offset, encoding, value in mutations:
            with self.subTest(offset=offset):
                data = bytearray(original)
                struct.pack_into(encoding, data, offset, value)
                data[entry + 32 : entry + 64] = hashlib.sha256(
                    data[end:-64]
                ).digest()
                data[-64:] = self.key.sign(
                    b"Blackflower.Pack.v1\0" + data[:end]
                )
                self._assert_rejected(bytes(data))

    def test_signing_failure_for_either_role_leaves_no_pair(self):
        for fail_at in (1, 2):
            calls = 0

            def signer(message, *, fail_at=fail_at):
                nonlocal calls
                calls += 1
                if calls == fail_at:
                    raise OSError("signing device unavailable")
                return self.key.sign(message)

            with self.subTest(role=fail_at), self.assertRaises(OSError):
                self._cook(signer)
            self.assertFalse(self.output.exists())
            self.assertFalse(self.output.with_name("pair.lock").exists())

    def test_failed_file_production_or_verification_leaves_no_pair(self):
        original_write = pathlib.Path.write_bytes
        original_read = pathlib.Path.read_bytes
        for role in ("client", "server"):

            def failed_write(path, data, *, role=role):
                if path.name == f"mvp.bf{role}":
                    raise OSError("storage failure")
                return original_write(path, data)

            with (
                self.subTest(stage="write", role=role),
                mock.patch.object(pathlib.Path, "write_bytes", failed_write),
                self.assertRaises(OSError),
            ):
                self._cook()
            self.assertFalse(self.output.exists())

            def corrupt_read(path, *, role=role):
                data = original_read(path)
                if path.name == f"mvp.bf{role}":
                    data = data[:-1] + bytes([data[-1] ^ 1])
                return data

            with (
                self.subTest(stage="verify", role=role),
                mock.patch.object(pathlib.Path, "read_bytes", corrupt_read),
                self.assertRaises((ValueError, exceptions.InvalidSignature)),
            ):
                self._cook()
            self.assertFalse(self.output.exists())
        with (
            mock.patch.object(
                pathlib.Path,
                "rename",
                side_effect=OSError("publication failed"),
            ),
            self.assertRaises(OSError),
        ):
            self._cook()
        self.assertFalse(self.output.exists())

    def test_recook_is_identical_and_existing_pair_is_preserved(self):
        first = self._cook()
        files = {path.name: path.read_bytes() for path in self.output.iterdir()}
        with self.assertRaises(ValueError):
            self._cook()
        self.assertEqual(
            files,
            {path.name: path.read_bytes() for path in self.output.iterdir()},
        )
        self.output = self.work / "second"
        self.assertEqual(first, self._cook())
        self.assertEqual(
            files,
            {path.name: path.read_bytes() for path in self.output.iterdir()},
        )

    def test_openusd_binary_input_and_geometry_validation(self):
        stage = Usd.Stage.Open(str(self.source))
        binary = self.work / "source.usdc"
        stage.GetRootLayer().Export(str(binary))
        self.source = binary
        self._cook()
        data = (self.output / "mvp.bfclient").read_bytes()
        self.assertEqual(
            pack.verify(data, 1, 1, [self.public]).payload[:8].hex(),
            "204e0000204e0000",
        )
        self.output = self.work / "invalid"
        stage.GetPrimAtPath("/Scenario/Spawns/SouthWest").GetAttribute(
            "xformOp:translate"
        ).Set((-3, 0, 0))
        stage.GetRootLayer().Export(str(binary))
        with self.assertRaisesRegex(ValueError, "spawn intersects box"):
            self._cook()
        self.assertFalse(self.output.exists())

    def test_openusd_rejects_external_composition_and_animation(self):
        stage = Usd.Stage.Open(str(self.source))
        stage.GetPrimAtPath(
            "/Scenario/Blocks/West"
        ).GetReferences().AddReference("missing.usda")
        stage.GetRootLayer().Save()
        with self.assertRaisesRegex(ValueError, "composition"):
            self._cook()
        stage.GetPrimAtPath(
            "/Scenario/Blocks/West"
        ).GetReferences().ClearReferences()
        stage.GetPrimAtPath("/Scenario/Blocks/West").GetAttribute("size").Set(
            2, Usd.TimeCode(1)
        )
        stage.GetRootLayer().Save()
        with self.assertRaisesRegex(ValueError, "animation"):
            self._cook()

    def test_openusd_rejects_unsupported_internal_references(self):
        original = self.source.read_bytes()
        for mechanism in (
            "inherit",
            "specialize",
            "relationship",
            "connection",
        ):
            with self.subTest(mechanism=mechanism):
                self.output = self.work / mechanism
                self.source.write_bytes(original)
                stage = Usd.Stage.Open(str(self.source))
                stage.GetRootLayer().Reload()
                prim = stage.GetPrimAtPath("/Scenario/Blocks/West")
                if mechanism == "inherit":
                    prim.GetInherits().AddInherit("/Missing")
                elif mechanism == "specialize":
                    prim.GetSpecializes().AddSpecialize("/Missing")
                elif mechanism == "relationship":
                    prim.CreateRelationship("target").SetTargets(["/Missing"])
                else:
                    prim.GetAttribute("size").AddConnection("/Missing.size")
                stage.GetRootLayer().Save()
                with self.assertRaisesRegex(ValueError, "unsupported USD"):
                    self._cook()
                self.assertFalse(self.output.exists())


if __name__ == "__main__":
    unittest.main()
