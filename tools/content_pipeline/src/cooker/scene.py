"""Encoding of scene entities and their independently authored bounds."""

import math
import re
import struct
from typing import TypedDict


class BoundData(TypedDict):
    """World-space oriented box, with metre dimensions and XYZW rotation."""

    center_m: list[float]
    dimensions_m: list[float]
    rotation_xyzw: list[float]


class EntityData(TypedDict):
    """Persistent identity, scene placement, and owned collision bounds."""

    id: str
    position_m: list[float]
    rotation_xyzw: list[float]
    scale: float
    bounds: list[BoundData]


class SceneData(TypedDict):
    """A scene consists only of entities."""

    entities: list[EntityData]


def encode(data: SceneData) -> bytes:
    """Encodes entities and bounds in scene v1 record order.

    Args:
        data: Entities sorted by their unique ASCII identities.

    Returns:
        Entity count followed by entity records and their bounds.

    Raises:
        ValueError: A field cannot be represented by this schema.
    """
    try:
        result = bytearray(struct.pack("<I", len(data["entities"])))
        for entity in data["entities"]:
            identity = entity["id"].encode("ascii")
            result.extend(struct.pack("<I", len(identity)))
            result.extend(identity)
            result.extend(
                struct.pack(
                    "<8dI",
                    *entity["position_m"],
                    *entity["rotation_xyzw"],
                    entity["scale"],
                    len(entity["bounds"]),
                )
            )
            for bound in entity["bounds"]:
                result.extend(
                    struct.pack(
                        "<I10d",
                        1,
                        *bound["center_m"],
                        *bound["dimensions_m"],
                        *bound["rotation_xyzw"],
                    )
                )
    except (struct.error, OverflowError) as error:
        raise ValueError("scene field cannot be encoded") from error
    return bytes(result)


def decode(payload: bytes) -> SceneData:
    """Decodes complete validated entities with no partial result.

    Args:
        payload: Scene v1 encoded bytes.

    Returns:
        Entities with their owned bounds, in encoded order.

    Raises:
        ValueError: Identity, geometry, record type or byte layout is invalid.
    """
    if len(payload) < 4:
        raise ValueError("invalid scene length")
    count = struct.unpack_from("<I", payload)[0]
    data: SceneData = {"entities": []}
    offset = 4
    for _ in range(count):
        entity, offset = _decode_entity(payload, offset)
        if data["entities"] and entity["id"] <= data["entities"][-1]["id"]:
            raise ValueError("entity IDs must be unique and sorted")
        data["entities"].append(entity)
    if offset != len(payload):
        raise ValueError("invalid scene length")
    return data


def _decode_entity(payload: bytes, offset: int) -> tuple[EntityData, int]:
    if offset + 4 > len(payload):
        raise ValueError("invalid entity length")
    size = struct.unpack_from("<I", payload, offset)[0]
    start, end = offset + 4, offset + 4 + size
    if end + 68 > len(payload):
        raise ValueError("invalid entity length")
    identity = payload[start:end].decode("ascii")
    if not re.fullmatch(r"[A-Za-z0-9][A-Za-z0-9_.:-]*", identity):
        raise ValueError("invalid entity identity")
    values = struct.unpack_from("<8dI", payload, end)
    position, rotation = list(values[:3]), list(values[3:7])
    _validate_transform(position, rotation, [values[7]])
    entity: EntityData = {
        "id": identity,
        "position_m": position,
        "rotation_xyzw": rotation,
        "scale": values[7],
        "bounds": [],
    }
    offset = end + 68
    for _ in range(values[8]):
        bound, offset = _decode_bound(payload, offset)
        entity["bounds"].append(bound)
    return entity, offset


def _decode_bound(payload: bytes, offset: int) -> tuple[BoundData, int]:
    if offset + 4 > len(payload):
        raise ValueError("invalid bound length")
    if struct.unpack_from("<I", payload, offset)[0] != 1:
        raise ValueError("unsupported bound kind")
    if offset + 84 > len(payload):
        raise ValueError("invalid bound length")
    values = list(struct.unpack_from("<10d", payload, offset + 4))
    _validate_transform(values[:3], values[6:], values[3:6])
    return {
        "center_m": values[:3],
        "dimensions_m": values[3:6],
        "rotation_xyzw": values[6:],
    }, offset + 84


def _validate_transform(
    position: list[float], rotation: list[float], dimensions: list[float]
) -> None:
    if (
        not all(math.isfinite(v) for v in position + rotation + dimensions)
        or any(v <= 0 for v in dimensions)
        or abs(sum(v * v for v in rotation) - 1) > 1e-12
    ):
        raise ValueError("invalid scene transform")
