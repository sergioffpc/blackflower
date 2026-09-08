"""Publish a pair only after verifying both completed role artifacts."""

from collections.abc import Callable
import pathlib
import tempfile

from blackflower_cooker import pack
from blackflower_cooker import scene


def cook_pair(
    source: pathlib.Path,
    output: pathlib.Path,
    public_key: bytes,
    sign: Callable[[bytes], bytes],
) -> dict[str, str]:
    """Publishes both verified role packs using a caller-owned signer.

    The output directory must not exist. Exceptions leave no newly published
    pair. Private signing-key ownership stays with the caller.

    Args:
        source: Self-contained OpenUSD scenario file, at most 64 KiB.
        output: New directory in which to publish the completed pair.
        public_key: Independently supplied raw Ed25519 public key.
        sign: Callback accepting transcript bytes and returning a signature.

    Returns:
        Client and server pack digests and the shared scenario build digest,
        encoded as hexadecimal strings.

    Raises:
        ValueError: The source, destination or completed pair is invalid.
        OSError: Reading, staging, reserving or publishing files fails.
        cryptography.exceptions.InvalidSignature: A completed signature fails
            verification.

    Exceptions raised by the signing callback propagate to the caller.
    """
    if output.exists():
        raise ValueError("output directory already exists")
    with source.open("rb") as stream:
        raw = stream.read(65537)
    if len(raw) > 65536:
        raise ValueError("source exceeds 64 KiB")
    payload = scene.encode(raw)
    provenance = pack.provenance(raw)
    resources = [(1, 1, payload), (2, 2, payload)]
    build_id = pack.build_identity(provenance, resources)
    output.parent.mkdir(parents=True, exist_ok=True)
    # The final directory must be new. An exclusive reservation prevents a
    # concurrent publisher from replacing even an empty destination directory.
    reservation = output.with_name(output.name + ".lock")
    with reservation.open("x"):
        try:
            if output.exists():
                raise ValueError("output directory already exists")
            with tempfile.TemporaryDirectory(
                prefix=".cook-", dir=output.parent
            ) as temporary:
                stage = pathlib.Path(temporary) / "pair"
                stage.mkdir()
                for role, profile, content in resources:
                    name = "client" if role == 1 else "server"
                    data = pack.encode(
                        content,
                        role,
                        profile,
                        provenance,
                        build_id,
                        public_key,
                        sign,
                    )
                    (stage / f"mvp.bf{name}").write_bytes(data)
                verified = []
                for role, profile, _ in resources:
                    name = "client" if role == 1 else "server"
                    data = (stage / f"mvp.bf{name}").read_bytes()
                    verified.append(
                        pack.verify(data, role, profile, [public_key])
                    )
                expected = pack.build_identity(
                    verified[0].provenance,
                    [(p.role, p.profile, p.payload) for p in verified],
                )
                if any(
                    p.build_id != expected
                    or p.provenance != provenance
                    or p.payload != payload
                    for p in verified
                ):
                    raise ValueError(
                        "completed pair disagrees on scenario build or scene"
                    )
                identities = {
                    "client": verified[0].pack_id.hex(),
                    "server": verified[1].pack_id.hex(),
                    "scenario_build_id": expected.hex(),
                }
                stage.rename(output)
                return identities
        finally:
            reservation.unlink()
