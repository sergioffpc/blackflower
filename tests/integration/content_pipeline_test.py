"""Production-to-consumption checks through the CLI and runtime harness."""

import json
import os
import pathlib
import subprocess
import sys
import tempfile
import unittest
from typing import Any

from cryptography.hazmat.primitives.asymmetric import ed25519
from cryptography.hazmat.primitives import serialization

from pxr import Sdf
from pxr import Usd
from pxr import UsdGeom

from content import pack

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


def _write_signing_keys(private: pathlib.Path, public: pathlib.Path) -> None:
    key = ed25519.Ed25519PrivateKey.generate()
    private.write_bytes(
        key.private_bytes(
            serialization.Encoding.PEM,
            serialization.PrivateFormat.PKCS8,
            serialization.NoEncryption(),
        )
    )
    public.write_bytes(
        key.public_key().public_bytes(
            serialization.Encoding.Raw, serialization.PublicFormat.Raw
        )
    )


def _write_authored_scene(source: pathlib.Path) -> None:
    stage = Usd.Stage.Open(str(ROOT / "assets/scenes/mvp.usda"))
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
            _write_signing_keys(private, public)
            source = work / "scene.usda"
            _write_authored_scene(source)
            output = work / "cooked"
            identities = self._cook(source, output, private)
            for name in ("server", "agent", "client"):
                self._check_cooked(work, output, public, identities, name)

    def _cook(
        self, source: pathlib.Path, output: pathlib.Path, private: pathlib.Path
    ) -> dict[str, str]:
        result = subprocess.run(
            [
                sys.executable,
                "-m",
                "content",
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


if __name__ == "__main__":
    unittest.main()
