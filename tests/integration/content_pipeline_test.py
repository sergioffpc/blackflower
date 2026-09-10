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
from typing import Any

from cryptography.hazmat.primitives.asymmetric import ed25519
from pxr import Sdf
from pxr import Usd
from pxr import UsdGeom

from cooker import pack

ROOT = pathlib.Path(__file__).resolve().parents[2]
HARNESS = pathlib.Path(
    os.environ.get(
        "BLACKFLOWER_CONTENT_HARNESS",
        ROOT / "build/debug/blackflower_content_harness",
    )
)


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


def _write_authored_scene(source: pathlib.Path) -> None:
    stage = Usd.Stage.Open(str(ROOT / "tests/integration/fixtures/mvp.usda"))
    _author_collision_shape(stage)
    _author_lights(stage)
    layer = stage.GetRootLayer()
    for name in ("NorthWest", "SouthEast", "NorthEast"):
        stage.RemovePrim(f"/Scenario/Spawns/{name}")
    stage.GetPrimAtPath("/Scenario/Spawns/SouthWest").GetAttribute(
        "xformOp:translate"
    ).Set((-3, 5, 0))
    # types-usd leaves the optional export-argument dictionary untyped.
    layer.Export(str(source))  # pyright: ignore[reportUnknownMemberType]


def _author_collision_shape(stage: Usd.Stage) -> None:
    sphere = UsdGeom.Sphere.Define(stage, "/Scenario/CollisionShapes/Ball")
    sphere.GetRadiusAttr().Set(0.5)
    sphere.GetPrim().CreateAttribute(
        "blackflower:id",
        stage.GetPrimAtPath("/Scenario/CollisionShapes/West")
        .GetAttribute("blackflower:id")
        .GetTypeName(),
    ).Set(8)
    UsdGeom.Xformable(sphere).AddTranslateOp().Set((1, 5, 2))


def _author_lights(stage: Usd.Stage) -> None:
    light = stage.GetPrimAtPath("/Scenario/Lights/Ceiling")
    light.GetAttribute("blackflower:color").Set((0.5, 0.25, 0.125))
    light.GetAttribute("blackflower:intensity").Set(2)
    directional = UsdGeom.Xform.Define(stage, "/Scenario/Lights/Sun").GetPrim()
    directional.CreateAttribute("blackflower:id", Sdf.ValueTypeNames.UInt).Set(
        2
    )
    directional.CreateAttribute(
        "blackflower:lightType", Sdf.ValueTypeNames.Token
    ).Set("directional")
    directional.CreateAttribute(
        "blackflower:direction", Sdf.ValueTypeNames.Float3
    ).Set((0, -1, 0))
    directional.CreateAttribute(
        "blackflower:color", Sdf.ValueTypeNames.Float3
    ).Set((1, 1, 1))
    directional.CreateAttribute(
        "blackflower:intensity", Sdf.ValueTypeNames.Float
    ).Set(3)


class ContentPipelineTest(unittest.TestCase):

    def test_cooked_pack_accepts_missing_collections(self):
        scopes = ("CollisionShapes", "Lights", "Spawns")
        with tempfile.TemporaryDirectory() as directory:
            work = pathlib.Path(directory)
            private, public = work / "private.pem", work / "public.key"
            self._generate_signing_keys(private, public)
            for missing in ((scope,) for scope in scopes):
                with self.subTest(missing=missing):
                    self._check_missing_collections(
                        work, private, public, missing
                    )
            with self.subTest(missing=scopes):
                self._check_missing_collections(work, private, public, scopes)

    def _check_missing_collections(
        self,
        work: pathlib.Path,
        private: pathlib.Path,
        public: pathlib.Path,
        missing: tuple[str, ...],
    ) -> None:
        stage = Usd.Stage.Open(
            str(ROOT / "tests/integration/fixtures/mvp.usda")
        )
        for scope in missing:
            stage.RemovePrim(f"/Scenario/{scope}")
        source = work / "scene.usda"
        stage.GetRootLayer().Export(str(source))
        output = work / "-".join(missing)
        self._cook(source, output, private)
        for name in ("server", "agent", "client"):
            loaded = subprocess.run(
                _harness_command(output / f"scene.bf{name}", public),
                capture_output=True,
                text=True,
                check=False,
            )
            self.assertEqual(loaded.returncode, 0, loaded.stderr)
            content = json.loads(loaded.stdout)
            self.assertEqual(
                len(content["collision_shapes"]),
                0 if "CollisionShapes" in missing else 7,
            )
            self.assertEqual(
                len(content["lights"]),
                1 if name == "client" and "Lights" not in missing else 0,
            )
            self.assertEqual(
                len(content["spawns"]),
                4 if name == "server" and "Spawns" not in missing else 0,
            )

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
        self.assertEqual(
            content["content_build_id"], reference["content_build_id"]
        )

    def test_cooked_pack_preserves_authored_scene(self):
        with tempfile.TemporaryDirectory() as directory:
            work = pathlib.Path(directory)
            private = work / "private.pem"
            public = work / "public.key"
            self._generate_signing_keys(private, public)
            source = work / "scene.usda"
            _write_authored_scene(source)
            output = work / "cooked"
            identities = self._cook(source, output, private)
            for name in ("server", "agent", "client"):
                self._check_cooked(work, output, public, identities, name)

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

    def _check_cooked(
        self,
        work: pathlib.Path,
        output: pathlib.Path,
        public: pathlib.Path,
        identities: dict[str, str],
        name: str,
    ) -> None:
        isolated = work / name
        isolated.mkdir()
        pack_path = isolated / "cenário.pack"
        pack_path.write_bytes((output / f"scene.bf{name}").read_bytes())
        loaded = subprocess.run(
            _harness_command(pack_path, public),
            capture_output=True,
            text=True,
            check=False,
            cwd=isolated,
        )
        self.assertEqual(loaded.returncode, 0, loaded.stderr)
        content = json.loads(loaded.stdout)
        self.assertEqual(content["scene_type"], name)
        self._check_collision_shapes(content)
        self._check_placements(content, name)
        self.assertEqual(
            content["content_build_id"],
            identities["content_build_id"],
        )

    def _check_collision_shapes(self, content: dict[str, Any]) -> None:
        self.assertEqual(len(content["collision_shapes"]), 8)
        self.assertEqual(
            content["collision_shapes"][0],
            {
                "id": 1,
                "kind": 1,
                "center_mm": [-3000, 1000, 0],
                "dimensions_mm": [2000, 2000, 2000],
            },
        )
        self.assertEqual(
            content["collision_shapes"][-1],
            {
                "id": 8,
                "kind": 2,
                "center_mm": [1000, 5000, 2000],
                "dimensions_mm": [500],
            },
        )

    def _check_placements(self, content: dict[str, Any], name: str) -> None:
        self.assertEqual(
            content["lights"],
            [
                {
                    "kind": 1,
                    "id": 1,
                    "position_mm": [0, 3000, 0],
                    "color": [0.5, 0.25, 0.125],
                    "intensity": 2,
                },
                {
                    "kind": 2,
                    "id": 2,
                    "direction": [0, -1, 0],
                    "color": [1, 1, 1],
                    "intensity": 3,
                },
            ]
            if name == "client"
            else [],
        )
        self.assertEqual(
            content["spawns"],
            [{"id": 1, "position_mm": [-3000, 5000, 0]}]
            if name == "server"
            else [],
        )


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
        self, payload: bytes = bytes(12), magic: bytes = b"BFAGNT1\0"
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
        for payload, diagnostic in (
            (b"", "invalid scene length"),
            (bytes(11), "invalid scene length"),
            (bytes(13), "invalid scene length"),
            (struct.pack("<3I", 2**32 - 1, 0, 0), "invalid scene length"),
            (struct.pack("<3I", 0, 2**32 - 1, 0), "invalid scene length"),
            (struct.pack("<3I", 0, 0, 2**32 - 1), "invalid scene length"),
            (
                struct.pack("<4I", 1, 0, 0, 257),
                "unsupported collision shape kind",
            ),
            (struct.pack("<4I", 0, 1, 0, 257), "unsupported light kind"),
        ):
            with self.subTest(payload=payload):
                self._reject(self._artifact(payload), diagnostic)

    def test_signed_records_cannot_be_truncated(self) -> None:
        for counts, record in (
            ((1, 0, 0), struct.pack("<8I", 1, 1, 0, 0, 0, 1, 1, 1)),
            ((1, 0, 0), struct.pack("<6I", 2, 1, 0, 0, 0, 1)),
            ((0, 1, 0), struct.pack("<9I", 1, 1, 0, 0, 0, 0, 0, 0, 0)),
            ((0, 1, 0), struct.pack("<9I", 2, 1, 0, 0, 0, 0, 0, 0, 0)),
            ((0, 0, 1), struct.pack("<4I", 1, 0, 0, 0)),
        ):
            magic, scene_type = (
                (b"BFCLNT1\0", "client")
                if counts[1]
                else (b"BFSERV1\0", "server")
                if counts[2]
                else (b"BFAGNT1\0", "agent")
            )
            header = struct.pack("<3I", *counts)
            self._accept(self._artifact(header + record, magic), scene_type)
            for size in range(len(record)):
                with self.subTest(counts=counts, size=size):
                    payload = header + record[:size]
                    self._reject(
                        self._artifact(payload, magic), "invalid scene length"
                    )

    def test_signed_scene_rejects_forbidden_collections(self) -> None:
        light = struct.pack("<3I", 0, 1, 0) + struct.pack(
            "<9I", 1, 1, 0, 0, 0, 0, 0, 0, 0
        )
        spawn = struct.pack("<3I", 0, 0, 1) + struct.pack("<4I", 1, 0, 0, 0)
        for magic, payload in (
            (b"BFSERV1\0", light),
            (b"BFAGNT1\0", light),
            (b"BFAGNT1\0", spawn),
            (b"BFCLNT1\0", spawn),
        ):
            with self.subTest(magic=magic, payload=payload):
                self._reject(
                    self._artifact(payload, magic), "invalid scene length"
                )


if __name__ == "__main__":
    unittest.main()
