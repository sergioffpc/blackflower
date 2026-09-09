"""Structural encoding of geometry, point lights and spawn points."""

import enum
import struct
from typing import Literal
from typing import TypedDict

from blackflower_cooker import usd_source


class GeometryKind(enum.IntEnum):
    """Geometry record discriminator in the scene encoding."""

    # Axis-aligned box with full extents.
    BOX = 1
    # Sphere with a centre and radius.
    SPHERE = 2


class GeometryData(TypedDict):
    """Geometry identity, kind, centre and dimensions in millimetres.

    Dimensions contains three full extents for boxes or one radius for spheres.
    """

    id: int
    kind: GeometryKind
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
    """Independent collections of geometry, lights and spawns."""

    geometries: list[GeometryData]
    lights: list[LightData]
    spawns: list[SpawnData]


def encode(source: bytes) -> bytes:
    """Encodes the supported OpenUSD scene subset without gameplay validation.

    Args:
        source: Exact bytes of a self-contained OpenUSD scene.

    Returns:
        A scene header followed by geometry, light and spawn records.

    Raises:
        ValueError: The source cannot be represented by this schema.
        OSError: Temporary source storage fails.
    """
    data = usd_source.read(source)
    try:
        result = struct.pack(
            "<3I",
            len(data["geometries"]),
            len(data["lights"]),
            len(data["spawns"]),
        )
        for geometry in data["geometries"]:
            encoding = (
                "<2I3i3I" if geometry["kind"] == GeometryKind.BOX else "<2I3iI"
            )
            result += struct.pack(
                encoding,
                geometry["kind"],
                geometry["id"],
                *geometry["center_mm"],
                *geometry["dimensions_mm"],
            )
        for light in data["lights"]:
            if light["kind"] == LightKind.POINT:
                result += struct.pack(
                    "<2I3i4f",
                    light["kind"],
                    light["id"],
                    *light["position_mm"],
                    *light["color"],
                    light["intensity"],
                )
            else:
                result += struct.pack(
                    "<2I7f",
                    light["kind"],
                    light["id"],
                    *light["direction"],
                    *light["color"],
                    light["intensity"],
                )
        for spawn in data["spawns"]:
            result += struct.pack("<I3i", spawn["id"], *spawn["position_mm"])
    except (struct.error, OverflowError) as error:
        raise ValueError("scene field cannot be encoded") from error
    return result


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
    geometry_count, lights, spawns = struct.unpack_from("<3I", payload)
    data: SceneData = {"geometries": [], "lights": [], "spawns": []}
    offset = 12
    for _ in range(geometry_count):
        if offset + 4 > len(payload):
            raise ValueError("invalid geometry record length")
        kind = GeometryKind(struct.unpack_from("<I", payload, offset)[0])
        encoding = struct.Struct(
            "<2I3i3I" if kind == GeometryKind.BOX else "<2I3iI"
        )
        if offset + encoding.size > len(payload):
            raise ValueError("invalid geometry record length")
        _, identity, *values = encoding.unpack_from(payload, offset)
        data["geometries"].append(
            {
                "kind": kind,
                "id": identity,
                "center_mm": values[:3],
                "dimensions_mm": values[3:],
            }
        )
        offset += encoding.size
    if offset + 36 * lights + 16 * spawns != len(payload):
        raise ValueError("invalid scene length")
    for _ in range(lights):
        light_kind = LightKind(struct.unpack_from("<I", payload, offset)[0])
        if light_kind == LightKind.POINT:
            _, identity, x, y, z, red, green, blue, intensity = (
                struct.unpack_from("<2I3i4f", payload, offset)
            )
            data["lights"].append(
                {
                    "kind": LightKind.POINT,
                    "id": identity,
                    "position_mm": [x, y, z],
                    "color": [red, green, blue],
                    "intensity": intensity,
                }
            )
        else:
            _, identity, dx, dy, dz, red, green, blue, intensity = (
                struct.unpack_from("<2I7f", payload, offset)
            )
            data["lights"].append(
                {
                    "kind": LightKind.DIRECTIONAL,
                    "id": identity,
                    "direction": [dx, dy, dz],
                    "color": [red, green, blue],
                    "intensity": intensity,
                }
            )
        offset += 36
    for _ in range(spawns):
        identity, *position = struct.unpack_from("<I3i", payload, offset)
        data["spawns"].append({"id": identity, "position_mm": position})
        offset += 16
    return data
