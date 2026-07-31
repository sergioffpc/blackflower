#!/usr/bin/env python3
"""Install the exact ISPC build required by the pinned Steam Audio SDK."""

from __future__ import annotations

import argparse
import hashlib
import platform
import shutil
import tarfile
import tempfile
import urllib.request
import zipfile
from pathlib import Path


VERSION = "1.12.0"
RELEASE_ROOT = f"https://github.com/ispc/ispc/releases/download/v{VERSION}"
ARTIFACTS = {
    "Linux": (
        "ispc-v1.12.0b-linux.tar.gz",
        "7a2bdd5fff5c1882639cfbd66bca31dbb68c7177f3013e80b0813a37fe0fdc23",
        "ispc-v1.12.0-linux/bin/ispc",
    ),
    "Darwin": (
        "ispc-v1.12.0-macOS.tar.gz",
        "e6c917b964e43218c422b46c9a6c71b876d88d0791da2ee3732b20a2e209c018",
        "ispc-v1.12.0-macOS/bin/ispc",
    ),
    "Windows": (
        "ispc-v1.12.0-windows.zip",
        "a35eb79c52456dfbd560edbfec99dae67f1beffd39b106922f5d02cd908c6454",
        "ispc-v1.12.0-windows/bin/ispc.exe",
    ),
}


def arguments() -> argparse.Namespace:
    parser = argparse.ArgumentParser()
    parser.add_argument("--output", required=True, type=Path)
    return parser.parse_args()


def digest(path: Path) -> str:
    result = hashlib.sha256()
    with path.open("rb") as source:
        for chunk in iter(lambda: source.read(1024 * 1024), b""):
            result.update(chunk)
    return result.hexdigest()


def extract(archive: Path, destination: Path) -> None:
    if archive.suffix == ".zip":
        with zipfile.ZipFile(archive) as source:
            source.extractall(destination)
    else:
        with tarfile.open(archive, "r:gz") as source:
            source.extractall(destination, filter="data")


def main() -> None:
    output = arguments().output.resolve()
    system = platform.system()
    try:
        filename, expected_digest, executable_relative = ARTIFACTS[system]
    except KeyError as error:
        raise SystemExit(f"ISPC {VERSION} is not configured for {system}") from error

    executable = output / ("ispc.exe" if system == "Windows" else "ispc")
    if executable.is_file():
        print(executable)
        return

    output.mkdir(parents=True, exist_ok=True)
    with tempfile.TemporaryDirectory(prefix="blackflower-ispc-") as temporary:
        temporary_path = Path(temporary)
        archive = temporary_path / filename
        urllib.request.urlretrieve(f"{RELEASE_ROOT}/{filename}", archive)
        actual_digest = digest(archive)
        if actual_digest != expected_digest:
            raise SystemExit(
                f"ISPC archive digest {actual_digest} does not match {expected_digest}"
            )
        extracted = temporary_path / "extracted"
        extracted.mkdir()
        extract(archive, extracted)
        shutil.copy2(extracted / executable_relative, executable)

    executable.chmod(0o755)
    print(executable)


if __name__ == "__main__":
    main()
