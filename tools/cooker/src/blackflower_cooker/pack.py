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

HEADER = struct.Struct("<8s3I3Q32s32s")
ENTRY = struct.Struct("<4I2Q32s")
PACK_DOMAIN = b"Blackflower.Pack.v1\0"
BUILD_DOMAIN = b"Blackflower.ScenarioBuild.v1\0"
SETTINGS = b"primitive-usd-v1;units=mm;simulation=1;presentation=2"


class Role(enum.IntEnum):
    """Purpose of content, independent of consumer platform or deployment."""

    # World rules and geometry shared by simulation and prediction.
    SIMULATION = 1
    # Resources used to render and present the scenario.
    PRESENTATION = 2


@dataclasses.dataclass(frozen=True)
class VerifiedPack:
    """An authenticated role pack with a validated primitive scene.

    Attributes:
        data: Complete signed pack bytes.
        role: Content purpose.
        provenance: Authenticated source, settings and toolchain provenance.
        build_id: Shared scenario build digest.
        pack_id: Digest of this role's signed transcript.
        payload: Validated scene bytes.
    """

    data: bytes
    role: Role
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


def build_identity(
    provenance_bytes: bytes, payloads: Sequence[tuple[Role, bytes]]
) -> bytes:
    """Computes the shared scenario identity in the supplied role order.

    Args:
        provenance_bytes: Canonical provenance record shared by both packs.
        payloads: Role/payload pairs in simulation then presentation order.

    Returns:
        The SHA-256 digest of the canonical scenario build transcript.
    """
    transcript = BUILD_DOMAIN + provenance_bytes
    for role, payload in payloads:
        transcript += struct.pack("<5IQ", role, 1, 1, 1, 1, len(payload))
        transcript += hashlib.sha256(payload).digest()
    return hashlib.sha256(transcript).digest()


def encode(
    payload: bytes,
    role: Role,
    provenance_bytes: bytes,
    build_id: bytes,
    public_key: bytes,
    sign: Callable[[bytes], bytes],
) -> bytes:
    """Encodes a role pack using a caller-owned Ed25519 signer.

    Args:
        payload: Encoded primitive scene.
        role: Content purpose.
        provenance_bytes: Canonical provenance record.
        build_id: Shared scenario build digest.
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
        b"BFPACK1\0",
        1,
        role,
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


def verify(
    data: bytes, role: Role, trusted_keys: Sequence[bytes]
) -> VerifiedPack:
    """Authenticates a pack and validates its layout and primitive scene.

    Args:
        data: Complete pack bytes.
        role: Required content purpose.
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
        actual_role,
        count,
        total,
        manifest_size,
        payload_size,
        key_id,
        build_id,
    ) = HEADER.unpack_from(data)
    if magic != b"BFPACK1\0" or version != 1:
        raise ValueError("unsupported pack format")
    if actual_role not in Role or actual_role != role:
        raise ValueError("wrong or unsupported pack role")
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
    scene.decode(payload)
    return VerifiedPack(
        data,
        role,
        provenance_bytes,
        build_id,
        hashlib.sha256(transcript).digest(),
        payload,
    )
