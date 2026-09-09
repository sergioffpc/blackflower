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

from blackflower_cooker import pack

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


if __name__ == "__main__":
    unittest.main()
