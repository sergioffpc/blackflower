"""Publish complete consumer scenes after verifying all signed artifacts."""

from collections.abc import Callable
import pathlib
import tempfile

from blackflower_cooker import pack
from blackflower_cooker import scene
from blackflower_cooker import usd_source


def _encode_scenes(source: bytes) -> dict[str, bytes]:
    data = usd_source.read(source)
    server_scene: scene.SceneData = {
        "collision_shapes": data["collision_shapes"],
        "lights": [],
        "spawns": data["spawns"],
    }
    agent_scene: scene.SceneData = {
        "collision_shapes": data["collision_shapes"],
        "lights": [],
        "spawns": [],
    }
    client_scene: scene.SceneData = {
        "collision_shapes": data["collision_shapes"],
        "lights": data["lights"],
        "spawns": [],
    }
    return {
        "server": scene.encode(server_scene),
        "agent": scene.encode(agent_scene),
        "client": scene.encode(client_scene),
    }


def cook(
    source: pathlib.Path,
    output: pathlib.Path,
    public_key: bytes,
    sign: Callable[[bytes], bytes],
) -> dict[str, str]:
    """Publishes all consumer scene packs using a caller-owned signer.

    The output directory must not exist. Exceptions leave no newly published
    pack set. Private signing-key ownership stays with the caller.

    Args:
        source: Self-contained OpenUSD scenario file.
        output: New directory in which to publish the completed pack set.
        public_key: Independently supplied raw Ed25519 public key.
        sign: Callback accepting transcript bytes and returning a signature.

    Returns:
        Server, agent, client and scenario build digests as hexadecimal
        strings.

    Raises:
        ValueError: The source, destination or completed pack set is invalid.
        OSError: Reading, staging, reserving or publishing files fails.
        cryptography.exceptions.InvalidSignature: A completed signature fails
            verification.

    Exceptions raised by the signing callback propagate to the caller.
    """
    if output.exists():
        raise ValueError("output directory already exists")
    with source.open("rb") as stream:
        raw = stream.read()
    payloads = _encode_scenes(raw)
    provenance = pack.provenance(raw)
    build_id = pack.build_identity(provenance, list(payloads.values()))
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
                stage = pathlib.Path(temporary) / "packs"
                stage.mkdir()
                for name, content in payloads.items():
                    data = pack.encode(
                        content,
                        provenance,
                        build_id,
                        public_key,
                        sign,
                        pack_type=pack.PackType[name.upper()],
                    )
                    (stage / f"{source.stem}.bf{name}").write_bytes(data)
                verified = {
                    name: pack.verify(
                        (stage / f"{source.stem}.bf{name}").read_bytes(),
                        [public_key],
                    )
                    for name in payloads
                }
                expected = pack.build_identity(
                    provenance, [value.payload for value in verified.values()]
                )
                if any(
                    value.pack_type != pack.PackType[name.upper()]
                    or value.build_id != expected
                    or value.provenance != provenance
                    or value.payload != payloads[name]
                    for name, value in verified.items()
                ):
                    raise ValueError(
                        "completed packs disagree on build or scene"
                    )
                identities = {
                    name: value.pack_id.hex()
                    for name, value in verified.items()
                }
                identities["scenario_build_id"] = expected.hex()
                stage.rename(output)
                return identities
        finally:
            reservation.unlink()
