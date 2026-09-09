"""Versioned signed pack encoding and verification using cryptography."""

from collections.abc import Callable
from collections.abc import Sequence
import dataclasses
import enum
import hashlib
import platform
import struct

import cryptography
from cryptography.hazmat.backends import openssl
from cryptography.hazmat.primitives.asymmetric import ed25519
from pxr import Usd

from blackflower_cooker import scene

HEADER = struct.Struct("<8s2I3Q32s32s")
ENTRY = struct.Struct("<4I2Q32s")
PACK_DOMAIN = b"Blackflower.Pack.v1\0"
BUILD_DOMAIN = b"Blackflower.ScenarioBuild.v1\0"
SETTINGS = b"primitive-usd-v1;units=mm;scenes=server,agent,client"


class PackType(enum.Enum):
    """File magic selecting the concrete scene contract."""

    # Authoritative collision geometry and spawn points.
    SERVER = b"BFSERV1\0"
    # Autonomous participant collision geometry.
    AGENT = b"BFAGNT1\0"
    # Human client collision and presentation data.
    CLIENT = b"BFCLNT1\0"


@dataclasses.dataclass(frozen=True)
class VerifiedPack:
    """An authenticated pack with a validated primitive scene.

    Attributes:
        data: Complete signed pack bytes.
        pack_type: Concrete file type authenticated by its magic.
        provenance: Authenticated source, settings and toolchain provenance.
        build_id: Scenario build digest.
        pack_id: Digest of the signed transcript.
        payload: Validated scene bytes.
    """

    data: bytes
    pack_type: PackType
    provenance: bytes
    build_id: bytes
    pack_id: bytes
    payload: bytes


def provenance(source: bytes) -> bytes:
    """Encodes source/settings digests and the active toolchain versions.

    Args:
        source: Exact source bytes consumed by the cooker.

    Returns:
        The canonical provenance record.

    Raises:
        ValueError: A version string cannot fit the provenance format.
    """
    result = hashlib.sha256(source).digest() + hashlib.sha256(SETTINGS).digest()
    for value in (
        "primitive-usd-v1",
        platform.python_version(),
        cryptography.__version__,
        openssl.backend.openssl_version_text(),
        ".".join(map(str, Usd.GetVersion())),
    ):
        raw = value.encode("ascii")
        if not raw or any(c < 32 or c > 126 for c in raw):
            raise ValueError("invalid provenance string")
        result += struct.pack("<I", len(raw)) + raw
    return result


def _validate_provenance(raw: bytes) -> None:
    if len(raw) < 64:
        raise ValueError("truncated provenance")
    cursor = 64
    for _ in range(5):
        if cursor + 4 > len(raw):
            raise ValueError("truncated provenance length")
        (size,) = struct.unpack_from("<I", raw, cursor)
        cursor += 4
        if size == 0 or cursor + size > len(raw):
            raise ValueError("invalid provenance length")
        if any(c < 32 or c > 126 for c in raw[cursor : cursor + size]):
            raise ValueError("invalid provenance text")
        cursor += size
    if cursor != len(raw):
        raise ValueError("trailing provenance bytes")


def build_identity(provenance_bytes: bytes, payloads: Sequence[bytes]) -> bytes:
    """Computes the scenario identity from provenance and resource contents.

    Args:
        provenance_bytes: Canonical provenance record.
        payloads: Encoded scene resources in server, agent then client order.

    Returns:
        The SHA-256 digest of the canonical scenario build transcript.
    """
    transcript = BUILD_DOMAIN + provenance_bytes
    for payload in payloads:
        transcript += struct.pack("<4IQ", 1, 1, 1, 1, len(payload))
        transcript += hashlib.sha256(payload).digest()
    return hashlib.sha256(transcript).digest()


def encode(
    payload: bytes,
    provenance_bytes: bytes,
    build_id: bytes,
    public_key: bytes,
    sign: Callable[[bytes], bytes],
    *,
    pack_type: PackType,
) -> bytes:
    """Encodes a pack using a caller-owned Ed25519 signer.

    Args:
        payload: Encoded primitive scene.
        pack_type: File type whose magic identifies the concrete scene contract.
        provenance_bytes: Canonical provenance record.
        build_id: Scenario build digest.
        public_key: Raw Ed25519 public key identifying the signer.
        sign: Callback accepting transcript bytes and returning a signature.

    Returns:
        The complete signed pack bytes.

    Raises:
        ValueError: The signer returns an invalid signature length.

    Exceptions raised by the signing callback propagate to the caller.
    """
    record = ENTRY.pack(
        1, 1, 1, 0, 0, len(payload), hashlib.sha256(payload).digest()
    )
    manifest = (
        struct.pack("<I", len(provenance_bytes)) + provenance_bytes + record
    )
    header = HEADER.pack(
        pack_type.value,
        1,
        1,
        HEADER.size + len(manifest) + len(payload) + 64,
        len(manifest),
        len(payload),
        hashlib.sha256(public_key).digest(),
        build_id,
    )
    signature = sign(PACK_DOMAIN + header + manifest)
    if len(signature) != 64:
        raise ValueError("signer returned an invalid signature size")
    return header + manifest + payload + signature


def verify(data: bytes, trusted_keys: Sequence[bytes]) -> VerifiedPack:
    """Authenticates a pack and validates its layout and primitive scene.

    Args:
        data: Complete pack bytes.
        trusted_keys: Independently provisioned raw Ed25519 public keys.

    Returns:
        The authenticated pack and validated payload.

    Raises:
        ValueError: Invalid layout, identity, provenance, digest or scene.
        cryptography.exceptions.InvalidSignature: Signature verification fails.
    """
    if len(data) < HEADER.size + 64:
        raise ValueError("invalid pack length")
    (
        magic,
        version,
        count,
        total,
        manifest_size,
        payload_size,
        key_id,
        build_id,
    ) = HEADER.unpack_from(data)
    pack_type = PackType(magic)
    if version != 1:
        raise ValueError("unsupported pack format")
    if count != 1 or manifest_size < 4 + ENTRY.size:
        raise ValueError("unsupported resource count or manifest size")
    payload_start = HEADER.size + manifest_size
    if total != len(data) or total != payload_start + payload_size + 64:
        raise ValueError("invalid pack layout")
    public = next(
        (
            key
            for key in trusted_keys
            if len(key) == 32 and hashlib.sha256(key).digest() == key_id
        ),
        None,
    )
    if public is None:
        raise ValueError("unknown signing key")
    transcript = PACK_DOMAIN + data[:payload_start]
    ed25519.Ed25519PublicKey.from_public_bytes(public).verify(
        data[-64:], transcript
    )
    (provenance_size,) = struct.unpack_from("<I", data, HEADER.size)
    if 4 + provenance_size + ENTRY.size != manifest_size:
        raise ValueError("invalid manifest layout")
    provenance_bytes = data[HEADER.size + 4 : HEADER.size + 4 + provenance_size]
    _validate_provenance(provenance_bytes)
    identity, kind, schema, reserved, offset, size, digest = ENTRY.unpack_from(
        data, HEADER.size + 4 + provenance_size
    )
    if (identity, kind, schema, reserved, offset, size) != (
        1,
        1,
        1,
        0,
        0,
        payload_size,
    ):
        raise ValueError("invalid resource identity, schema or range")
    payload = data[payload_start:-64]
    if hashlib.sha256(payload).digest() != digest:
        raise ValueError("resource digest mismatch")
    decoded = scene.decode(payload)
    if (pack_type != PackType.CLIENT and decoded["lights"]) or (
        pack_type != PackType.SERVER and decoded["spawns"]
    ):
        raise ValueError("collections are incompatible with the scene type")
    return VerifiedPack(
        data,
        pack_type,
        provenance_bytes,
        build_id,
        hashlib.sha256(transcript).digest(),
        payload,
    )
