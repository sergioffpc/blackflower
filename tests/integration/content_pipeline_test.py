"""Production-to-consumption checks through the CLI and runtime harness."""

import json
import os
import pathlib
import subprocess
import sys
import tempfile
import unittest

from cryptography.hazmat.primitives.asymmetric import ed25519
from cryptography.hazmat.primitives import serialization

from pxr import Sdf
from pxr import Usd
from pxr import UsdGeom

from blackflower_cooker import pack

ROOT = pathlib.Path(__file__).resolve().parents[2]
HARNESS = pathlib.Path(
    os.environ.get(
        "BLACKFLOWER_CONTENT_HARNESS",
        ROOT / "build/debug/blackflower_content_harness",
    )
)


def _harness_command(pack_path, role, public):
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
            for value in pack.Role:
                role = value.name.lower()
                data = (
                    ROOT / f"tests/fixtures/packs/reference.bf{role}"
                ).read_bytes()
                verified = pack.verify(data, value, [public])
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

    def test_each_role_reads_an_independent_scene_without_its_counterpart(self):
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
            source = work / "scene.usda"
            stage = Usd.Stage.Open(str(ROOT / "assets/scenes/mvp.usda"))
            sphere = UsdGeom.Sphere.Define(stage, "/Scenario/Geometry/Ball")
            sphere.GetRadiusAttr().Set(0.5)
            sphere.GetPrim().CreateAttribute(
                "blackflower:id",
                stage.GetPrimAtPath("/Scenario/Geometry/West")
                .GetAttribute("blackflower:id")
                .GetTypeName(),
            ).Set(8)
            UsdGeom.Xformable(sphere).AddTranslateOp().Set((1, 5, 2))
            light = stage.GetPrimAtPath("/Scenario/Lights/Ceiling")
            light.GetAttribute("blackflower:color").Set((0.5, 0.25, 0.125))
            light.GetAttribute("blackflower:intensity").Set(2)
            directional = UsdGeom.Xform.Define(
                stage, "/Scenario/Lights/Sun"
            ).GetPrim()
            directional.CreateAttribute(
                "blackflower:id", Sdf.ValueTypeNames.UInt
            ).Set(2)
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
            layer = stage.GetRootLayer()
            for name in ("NorthWest", "SouthEast", "NorthEast"):
                stage.RemovePrim(f"/Scenario/Spawns/{name}")
            stage.GetPrimAtPath("/Scenario/Spawns/SouthWest").GetAttribute(
                "xformOp:translate"
            ).Set((-3, 5, 0))
            layer.Export(str(source))
            output = work / "pair"
            result = subprocess.run(
                [
                    sys.executable,
                    "-m",
                    "blackflower_cooker",
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
            self.assertEqual(
                sorted(p.name for p in output.iterdir()),
                ["mvp.bfpresentation", "mvp.bfsimulation"],
            )
            results = []
            for role in ("simulation", "presentation"):
                isolated = work / role
                isolated.mkdir()
                pack_path = isolated / f"cenário.bf{role}"
                pack_path.write_bytes((output / f"mvp.bf{role}").read_bytes())
                loaded = subprocess.run(
                    _harness_command(pack_path, role, public),
                    capture_output=True,
                    text=True,
                    check=False,
                    cwd=isolated,
                )
                self.assertEqual(loaded.returncode, 0, loaded.stderr)
                content = json.loads(loaded.stdout)
                self.assertEqual(len(content["geometries"]), 8)
                self.assertEqual(
                    content["geometries"][0],
                    {
                        "id": 1,
                        "kind": 1,
                        "center_mm": [-3000, 1000, 0],
                        "dimensions_mm": [2000, 2000, 2000],
                    },
                )
                self.assertEqual(
                    content["geometries"][-1],
                    {
                        "id": 8,
                        "kind": 2,
                        "center_mm": [1000, 5000, 2000],
                        "dimensions_mm": [500],
                    },
                )
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
                    ],
                )
                self.assertEqual(
                    content["spawns"],
                    [{"id": 1, "position_mm": [-3000, 5000, 0]}],
                )
                results.append(content)
            self.assertNotEqual(results[0]["pack_id"], results[1]["pack_id"])
            self.assertEqual(
                results[0]["scenario_build_id"], results[1]["scenario_build_id"]
            )


if __name__ == "__main__":
    unittest.main()
