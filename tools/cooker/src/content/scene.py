"""Structural encoding of collision shapes, lights and spawn points."""

import enum
import struct
from typing import Literal
from typing import TypedDict


class CollisionShapeKind(enum.IntEnum):
    """CollisionShape record discriminator in the scene encoding."""

    # Axis-aligned box with full extents.
    BOX = 1
    # Sphere with a centre and radius.
    SPHERE = 2


class CollisionShapeData(TypedDict):
    """CollisionShape identity, kind, centre and dimensions in millimetres.

    Dimensions contains three full extents for boxes or one radius for spheres.
    """

    id: int
    kind: CollisionShapeKind
    center_mm: list[int]
    dimensions_mm: list[int]


class LightKind(enum.IntEnum):
    """Light record discriminator in the scene encoding."""

    # Omnidirectional emitter at a position.
    POINT = 1
    # Parallel emitter with a scene-space direction of travel.
    DIRECTIONAL = 2


class PointLightData(TypedDict):
    """Point emitter with linear RGB color and dimensionless intensity."""

    kind: Literal[LightKind.POINT]
    id: int
    position_mm: list[int]
    color: list[float]
    intensity: float


class DirectionalLightData(TypedDict):
    """Parallel emitter; direction is normalized by the consumer."""

    kind: Literal[LightKind.DIRECTIONAL]
    id: int
    direction: list[float]
    color: list[float]
    intensity: float


LightData = PointLightData | DirectionalLightData


class SpawnData(TypedDict):
    """Placement origin in millimetres; consumers define what is spawned."""

    id: int
    position_mm: list[int]


class SceneData(TypedDict):
    """Independent collections of collision shapes, lights and spawns."""

    collision_shapes: list[CollisionShapeData]
    lights: list[LightData]
    spawns: list[SpawnData]


def encode(data: SceneData) -> bytes:
    """Encodes scene collections without gameplay validation.

    Args:
        data: CollisionShape, light and spawn collections to serialize.

    Returns:
        A scene header followed by collision shape, light and spawn records.

    Raises:
        ValueError: A field cannot be represented by this schema.
    """
    try:
        result = struct.pack(
            "<3I",
            len(data["collision_shapes"]),
            len(data["lights"]),
            len(data["spawns"]),
        )
        for collision_shape in data["collision_shapes"]:
            result += _encode_collision_shape(collision_shape)
        for light in data["lights"]:
            result += _encode_light(light)
        for spawn in data["spawns"]:
            result += struct.pack("<I3i", spawn["id"], *spawn["position_mm"])
    except (struct.error, OverflowError) as error:
        raise ValueError("scene field cannot be encoded") from error
    return result


def _encode_collision_shape(shape: CollisionShapeData) -> bytes:
    encoding = (
        "<2I3i3I" if shape["kind"] == CollisionShapeKind.BOX else "<2I3iI"
    )
    return struct.pack(
        encoding,
        shape["kind"],
        shape["id"],
        *shape["center_mm"],
        *shape["dimensions_mm"],
    )


def _encode_light(light: LightData) -> bytes:
    if light["kind"] == LightKind.POINT:
        return struct.pack(
            "<2I3i4f",
            light["kind"],
            light["id"],
            *light["position_mm"],
            *light["color"],
            light["intensity"],
        )
    return struct.pack(
        "<2I7f",
        light["kind"],
        light["id"],
        *light["direction"],
        *light["color"],
        light["intensity"],
    )


def decode(payload: bytes) -> SceneData:
    """Checks record boundaries and decodes scene data without geometry rules.

    Args:
        payload: Encoded scene bytes.

    Returns:
        Collections in their encoded order.

    Raises:
        ValueError: Record types or lengths do not match the scene schema.
    """
    if len(payload) < 12:
        raise ValueError("invalid scene length")
    collision_shape_count, lights, spawns = struct.unpack_from("<3I", payload)
    data: SceneData = {"collision_shapes": [], "lights": [], "spawns": []}
    offset = 12
    for _ in range(collision_shape_count):
        shape, offset = _decode_collision_shape(payload, offset)
        data["collision_shapes"].append(shape)
    if offset + 36 * lights + 16 * spawns != len(payload):
        raise ValueError("invalid scene length")
    for _ in range(lights):
        data["lights"].append(_decode_light(payload, offset))
        offset += 36
    for _ in range(spawns):
        identity, *position = struct.unpack_from("<I3i", payload, offset)
        data["spawns"].append({"id": identity, "position_mm": position})
        offset += 16
    return data


def _decode_collision_shape(
    payload: bytes, offset: int
) -> tuple[CollisionShapeData, int]:
    if offset + 4 > len(payload):
        raise ValueError("invalid collision shape record length")
    kind = CollisionShapeKind(struct.unpack_from("<I", payload, offset)[0])
    encoding = struct.Struct(
        "<2I3i3I" if kind == CollisionShapeKind.BOX else "<2I3iI"
    )
    if offset + encoding.size > len(payload):
        raise ValueError("invalid collision shape record length")
    _, identity, *values = encoding.unpack_from(payload, offset)
    return {
        "kind": kind,
        "id": identity,
        "center_mm": values[:3],
        "dimensions_mm": values[3:],
    }, offset + encoding.size


def _decode_light(payload: bytes, offset: int) -> LightData:
    kind = LightKind(struct.unpack_from("<I", payload, offset)[0])
    if kind == LightKind.POINT:
        _, identity, x, y, z, red, green, blue, intensity = struct.unpack_from(
            "<2I3i4f", payload, offset
        )
        return {
            "kind": LightKind.POINT,
            "id": identity,
            "position_mm": [x, y, z],
            "color": [red, green, blue],
            "intensity": intensity,
        }
    _, identity, dx, dy, dz, red, green, blue, intensity = struct.unpack_from(
        "<2I7f", payload, offset
    )
    return {
        "kind": LightKind.DIRECTIONAL,
        "id": identity,
        "direction": [dx, dy, dz],
        "color": [red, green, blue],
        "intensity": intensity,
    }
