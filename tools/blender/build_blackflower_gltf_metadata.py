#!/usr/bin/env python3

"""Build a deterministic Blender extension archive."""

from __future__ import annotations

import argparse
from pathlib import Path
import tomllib
from zipfile import ZIP_DEFLATED, ZipFile, ZipInfo


SOURCE = Path(__file__).with_name("blackflower_gltf_metadata")
PACKAGE_FILES = (
    "__init__.py",
    "metadata.py",
    "blender_manifest.toml",
    "README.md",
)
ARCHIVE_TIMESTAMP = (1980, 1, 1, 0, 0, 0)


def main() -> None:
    manifest = tomllib.loads((SOURCE / "blender_manifest.toml").read_text())
    default_output = (
        Path(__file__).parents[2]
        / "target"
        / "blender"
        / f"{manifest['id']}-{manifest['version']}.zip"
    )
    parser = argparse.ArgumentParser(description=__doc__)
    parser.add_argument(
        "--output",
        type=Path,
        default=default_output,
        help=f"archive path (default: {default_output})",
    )
    arguments = parser.parse_args()

    arguments.output.parent.mkdir(parents=True, exist_ok=True)
    with ZipFile(
        arguments.output,
        "w",
        compression=ZIP_DEFLATED,
        compresslevel=9,
    ) as archive:
        for name in PACKAGE_FILES:
            source = SOURCE / name
            entry = ZipInfo(name, ARCHIVE_TIMESTAMP)
            entry.compress_type = ZIP_DEFLATED
            entry.external_attr = 0o100644 << 16
            archive.writestr(entry, source.read_bytes())

    print(arguments.output)


if __name__ == "__main__":
    main()
