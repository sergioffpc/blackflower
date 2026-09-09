"""Linux cooker command line."""

import argparse
import json
import os
import pathlib
import sys

from cryptography import exceptions
from cryptography.hazmat.primitives.asymmetric import ed25519
from cryptography.hazmat.primitives import serialization

from cooker import pipeline


def _write_key(path: pathlib.Path, data: bytes) -> None:
    """Creates a key file accessible only to its owner, without replacement."""
    descriptor = os.open(path, os.O_WRONLY | os.O_CREAT | os.O_EXCL, 0o600)
    try:
        with os.fdopen(descriptor, "wb") as stream:
            stream.write(data)
    except OSError:
        path.unlink()
        raise


def _generate_keys(
    private: pathlib.Path, public: pathlib.Path
) -> dict[str, str]:
    """Creates a new Ed25519 pair in the cooker's and runtime's key formats."""
    key = ed25519.Ed25519PrivateKey.generate()
    private_bytes = key.private_bytes(
        serialization.Encoding.PEM,
        serialization.PrivateFormat.PKCS8,
        serialization.NoEncryption(),
    )
    public_bytes = key.public_key().public_bytes(
        serialization.Encoding.Raw, serialization.PublicFormat.Raw
    )
    _write_key(private, private_bytes)
    try:
        _write_key(public, public_bytes)
    except OSError:
        private.unlink()
        raise
    return {"privateKey": str(private), "publicKey": str(public)}


def _cook(args: argparse.Namespace) -> dict[str, str]:
    """Loads the signing key and cooks the requested scene."""
    key = serialization.load_pem_private_key(
        args.private_key.read_bytes(), password=None
    )
    if not isinstance(key, ed25519.Ed25519PrivateKey):
        raise ValueError("content signing requires an Ed25519 key")
    public = key.public_key().public_bytes(
        serialization.Encoding.Raw, serialization.PublicFormat.Raw
    )
    return pipeline.cook(args.source, args.output, public, key.sign)


def _parse_args() -> argparse.Namespace:
    """Parses the requested cooker operation."""
    parser = argparse.ArgumentParser(
        description="Generate signing keys and cook signed scenes"
    )
    commands = parser.add_subparsers(dest="command", required=True)
    cook = commands.add_parser("cook", help="cook and sign scene packs")
    cook.add_argument("--source", type=pathlib.Path, required=True)
    cook.add_argument("--output", type=pathlib.Path, required=True)
    cook.add_argument("--private-key", type=pathlib.Path, required=True)
    keygen = commands.add_parser("keygen", help="create an Ed25519 key pair")
    keygen.add_argument(
        "--private-key",
        type=pathlib.Path,
        required=True,
        help="new unencrypted PEM/PKCS8 signing key file",
    )
    keygen.add_argument(
        "--public-key",
        type=pathlib.Path,
        required=True,
        help="new raw 32-byte verification key file",
    )
    return parser.parse_args()


def main() -> int:
    """Runs the cooker command line and returns its process exit status."""
    args = _parse_args()
    try:
        if sys.platform != "linux":
            raise ValueError("the offline cooker requires Linux")
        if args.command == "keygen":
            result = _generate_keys(args.private_key, args.public_key)
        else:
            result = _cook(args)
    except (
        OSError,
        ValueError,
        TypeError,
        exceptions.InvalidSignature,
        exceptions.UnsupportedAlgorithm,
    ) as error:
        print(
            f"{args.command} failed: {error or type(error).__name__}",
            file=sys.stderr,
        )
        return 1
    print(json.dumps(result, sort_keys=True))
    return 0


if __name__ == "__main__":
    sys.exit(main())
