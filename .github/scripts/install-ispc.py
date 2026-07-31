#!/usr/bin/env python3
"""Install Blackflower's pinned ISPC compiler from an official release archive."""

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


VERSION = "1.31.0"
RELEASE_ROOT = f"https://github.com/ispc/ispc/releases/download/v{VERSION}"
ARTIFACTS = {
    ("Linux", "x86_64"): (
        "ispc-v1.31.0-linux.tar.gz",
        "d74089c835e10fd7e2c4b9225ced38b87d1fb53d35c7ceabd48cdf035da11b11",
        "ispc-v1.31.0-linux/bin/ispc",
    ),
    ("Linux", "aarch64"): (
        "ispc-v1.31.0-linux.aarch64.tar.gz",
        "660ccac47ff7e0980b89b00a3ebd70201acf55f9e816c127fc28e868ab456193",
        "ispc-v1.31.0-linux.aarch64/bin/ispc",
    ),
    ("Darwin", "x86_64"): (
        "ispc-v1.31.0-macOS.x86_64.tar.gz",
        "ab800e62acb8fe95c07c501e986a51ed14a839090c0e8105bd8e75df2b095eab",
        "ispc-v1.31.0-macOS.x86_64/bin/ispc",
    ),
    ("Darwin", "aarch64"): (
        "ispc-v1.31.0-macOS.arm64.tar.gz",
        "eac8009da38d41074d0adcf1fad4a3412fc9644a81ee5a49efeb07eac505b6ec",
        "ispc-v1.31.0-macOS.arm64/bin/ispc",
    ),
    ("Windows", "x86_64"): (
        "ispc-v1.31.0-windows.zip",
        "9a18793800b91d5be7b851513672cd9a81a985a5a5dfec5611c2318e8ad4140a",
        "ispc-v1.31.0-windows/bin/ispc.exe",
    ),
}


def architecture() -> str:
    machine = platform.machine().lower()
    if machine in {"amd64", "x64", "x86_64"}:
        return "x86_64"
    if machine in {"aarch64", "arm64"}:
        return "aarch64"
    return machine


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
    machine = architecture()
    try:
        filename, expected_digest, executable_relative = ARTIFACTS[(system, machine)]
    except KeyError as error:
        raise SystemExit(
            f"ISPC {VERSION} is not configured for {system} {machine}"
        ) from error

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
