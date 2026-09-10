"""Publish complete consumer scenes after verifying all signed artifacts."""

from collections.abc import Callable
import enum
import pathlib
import tempfile

from cooker import pack
from cooker import scene
from cooker import usd_source


class CookStage(enum.IntEnum):
    """Ordered work boundaries; COMPLETE means publication succeeded."""

    READING = 0
    ENCODING = 1
    SIGNING = 2
    VERIFYING = 3
    PUBLISHING = 4
    COMPLETE = 5


def _encode_scenes(data: scene.SceneData) -> dict[str, bytes]:
    payload = scene.encode(data)
    return {name: payload for name in ("server", "agent", "client")}


def cook(
    source: pathlib.Path,
    output: pathlib.Path,
    public_key: bytes,
    sign: Callable[[bytes], bytes],
    progress: Callable[[CookStage], None] = lambda stage: None,
) -> dict[str, str]:
    """Publishes all consumer scene packs using a caller-owned signer.

    The output directory must not exist. Exceptions leave no newly published
    pack set. Private signing-key ownership stays with the caller.

    Args:
        source: OpenUSD scene with relative entity definition references.
        output: New directory in which to publish the completed pack set.
        public_key: Independently supplied raw Ed25519 public key.
        sign: Callback accepting transcript bytes and returning a signature.
        progress: Observer called at stage boundaries; must not raise.

    Returns:
        The common content build digest as a hexadecimal string in JSON data.

    Raises:
        ValueError: The source, destination or completed pack set is invalid.
        OSError: Reading, staging, reserving or publishing files fails.
        cryptography.exceptions.InvalidSignature: A completed signature fails
            verification.

    Exceptions raised by the signing callback propagate to the caller.
    """
    if output.exists():
        raise ValueError("output directory already exists")
    progress(CookStage.READING)
    data, raw = usd_source.read(source)
    progress(CookStage.ENCODING)
    payloads = _encode_scenes(data)
    provenance = pack.provenance(raw)
    content_build_id = pack.build_identity(provenance, list(payloads.values()))
    progress(CookStage.SIGNING)
    result = _publish(
        output,
        source.stem,
        payloads,
        provenance,
        content_build_id,
        public_key,
        sign,
        progress,
    )
    progress(CookStage.COMPLETE)
    return result


def _publish(
    output: pathlib.Path,
    stem: str,
    payloads: dict[str, bytes],
    provenance: bytes,
    content_build_id: bytes,
    public_key: bytes,
    sign: Callable[[bytes], bytes],
    progress: Callable[[CookStage], None],
) -> dict[str, str]:
    output.parent.mkdir(parents=True, exist_ok=True)
    # An exclusive reservation prevents concurrent destination replacement.
    reservation = output.with_name(output.name + ".lock")
    with reservation.open("x"):
        try:
            if output.exists():
                raise ValueError("output directory already exists")
            with tempfile.TemporaryDirectory(
                prefix=".cook-", dir=output.parent
            ) as temporary:
                stage = pathlib.Path(temporary) / "packs"
                stage.mkdir()
                _write_packs(
                    stage,
                    stem,
                    payloads,
                    provenance,
                    content_build_id,
                    public_key,
                    sign,
                )
                progress(CookStage.VERIFYING)
                identities = _verify_staged(
                    stage, stem, payloads, provenance, public_key
                )
                progress(CookStage.PUBLISHING)
                stage.rename(output)
                return identities
        finally:
            reservation.unlink()


def _write_packs(
    stage: pathlib.Path,
    stem: str,
    payloads: dict[str, bytes],
    provenance: bytes,
    content_build_id: bytes,
    public_key: bytes,
    sign: Callable[[bytes], bytes],
) -> None:
    for name, payload in payloads.items():
        data = pack.encode_and_sign(
            payload,
            provenance,
            content_build_id,
            public_key,
            sign,
            pack_type=pack.PackType[name.upper()],
        )
        (stage / f"{stem}.bf{name}").write_bytes(data)


def _verify_staged(
    stage: pathlib.Path,
    stem: str,
    payloads: dict[str, bytes],
    provenance: bytes,
    public_key: bytes,
) -> dict[str, str]:
    verified = {
        name: pack.verify(
            (stage / f"{stem}.bf{name}").read_bytes(), [public_key]
        )
        for name in payloads
    }
    expected = pack.build_identity(
        provenance, [value.payload for value in verified.values()]
    )
    if any(
        value.pack_type != pack.PackType[name.upper()]
        or value.content_build_id != expected
        or value.provenance != provenance
        or value.payload != payloads[name]
        for name, value in verified.items()
    ):
        raise ValueError("completed packs disagree on build or scene")
    return {"content_build_id": expected.hex()}
