"""Linux cooker command line."""

import argparse
import json
import pathlib
import sys

from cryptography import exceptions
from cryptography.hazmat.primitives.asymmetric import ed25519
from cryptography.hazmat.primitives import serialization

from blackflower_cooker import pipeline


def main() -> int:
    """Runs the cooker command line and returns its process exit status."""
    parser = argparse.ArgumentParser(
        description="Cook and sign server, agent and client scenes"
    )
    commands = parser.add_subparsers(dest="command", required=True)
    cook = commands.add_parser("cook")
    cook.add_argument("--source", type=pathlib.Path, required=True)
    cook.add_argument("--output", type=pathlib.Path, required=True)
    cook.add_argument("--private-key", type=pathlib.Path, required=True)
    args = parser.parse_args()
    try:
        if sys.platform != "linux":
            raise ValueError("the offline cooker requires Linux")
        key = serialization.load_pem_private_key(
            args.private_key.read_bytes(), password=None
        )
        if not isinstance(key, ed25519.Ed25519PrivateKey):
            raise ValueError("content signing requires an Ed25519 key")
        public = key.public_key().public_bytes(
            serialization.Encoding.Raw, serialization.PublicFormat.Raw
        )
        result = pipeline.cook(args.source, args.output, public, key.sign)
    except (
        OSError,
        ValueError,
        TypeError,
        exceptions.InvalidSignature,
        exceptions.UnsupportedAlgorithm,
    ) as error:
        print(f"cook failed: {error or type(error).__name__}", file=sys.stderr)
        return 1
    print(json.dumps(result, sort_keys=True))
    return 0


if __name__ == "__main__":
    sys.exit(main())
