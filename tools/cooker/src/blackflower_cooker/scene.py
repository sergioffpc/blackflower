"""Fixed scenario source validation and primitive encoding."""

from collections.abc import Sequence
import struct
from typing import TypedDict

from blackflower_cooker import usd_source


class BoxData(TypedDict):
    """A static box identity, center and dimensions in millimetres."""

    id: int
    center_mm: list[int]
    size_mm: list[int]


class SpawnData(TypedDict):
    """A spawn identity and foot position in millimetres."""

    id: int
    foot_mm: list[int]


class SceneData(TypedDict):
    """The primitive scenario schema, dimensions, boxes and spawns."""

    schema: int
    scenario: str
    interior_mm: list[int]
    wall_mm: list[int]
    capsule_mm: list[int]
    boxes: list[BoxData]
    spawns: list[SpawnData]


def _fields(value: object, names: str) -> None:
    if not isinstance(value, dict) or set(value) != set(names.split()):
        raise ValueError(f"expected fields: {names}")


def _numbers(value: object, count: int) -> list[int]:
    if not isinstance(value, list) or len(value) != count:
        raise ValueError(f"expected {count} coordinates/dimensions")
    # Reject bool and int subclasses: the schema requires exact integers.
    # pylint: disable-next=unidiomatic-typecheck
    if any(type(n) is not int or abs(n) > 20000 for n in value):
        raise ValueError("dimensions must be integer millimetres within 20000")
    return value


def _identities(
    items: Sequence[BoxData] | Sequence[SpawnData], fields: str
) -> None:
    previous = 0
    for item in items:
        _fields(item, fields)
        identity = item["id"]
        # Reject bool and int subclasses for canonical numeric identities.
        # pylint: disable-next=unidiomatic-typecheck
        if type(identity) is not int or not previous < identity <= 0xFFFFFFFF:
            raise ValueError("identities must be positive, unique and sorted")
        previous = identity


def validate(scene: SceneData) -> SceneData:
    """Validates dimensions, identities, ground support and separation.

    Args:
        scene: Parsed primitive scenario to validate.

    Returns:
        The original scene after successful validation.

    Raises:
        ValueError: A scenario field or geometric constraint is invalid.
    """
    _fields(
        scene, "schema scenario interior_mm wall_mm capsule_mm boxes spawns"
    )
    if (
        # A boolean must not be accepted as schema version 1.
        type(scene["schema"]) is not int  # pylint: disable=unidiomatic-typecheck
        or scene["schema"] != 1
        or scene["scenario"] != "mvp"
    ):
        raise ValueError("unsupported scenario schema or identity")
    if _numbers(scene["interior_mm"], 2) != [20000, 20000]:
        raise ValueError("interior must be 20000 by 20000 mm")
    if _numbers(scene["capsule_mm"], 2) != [1800, 600]:
        raise ValueError("capsule must be 1800 mm high and 600 mm in diameter")
    height, thickness = _numbers(scene["wall_mm"], 2)
    if height < 1800 or thickness <= 0:
        raise ValueError("invalid wall dimensions")
    boxes, spawns = scene["boxes"], scene["spawns"]
    if (
        not isinstance(boxes, list)
        or not isinstance(spawns, list)
        or len(boxes) != 2
        or len(spawns) != 4
    ):
        raise ValueError("scenario requires two boxes and four spawns")
    _identities(boxes, "id center_mm size_mm")
    _identities(spawns, "id foot_mm")
    for box in boxes:
        center = _numbers(box["center_mm"], 3)
        size = _numbers(box["size_mm"], 3)
        if any(n <= 0 or n % 2 for n in size) or center[1] * 2 != size[1]:
            raise ValueError(
                "boxes require positive even sizes and ground support"
            )
        if any(abs(center[i]) + size[i] // 2 > 10000 for i in (0, 2)):
            raise ValueError("box outside interior")
    a, b = boxes
    if all(
        abs(a["center_mm"][i] - b["center_mm"][i]) * 2
        < a["size_mm"][i] + b["size_mm"][i]
        for i in range(3)
    ):
        raise ValueError("boxes overlap")
    for index, spawn in enumerate(spawns):
        foot = _numbers(spawn["foot_mm"], 3)
        if foot[1] != 0 or any(abs(foot[i]) + 300 >= 10000 for i in (0, 2)):
            raise ValueError("spawn outside valid ground area")
        for box in boxes:
            distance = sum(
                max(
                    abs(foot[i] - box["center_mm"][i]) - box["size_mm"][i] // 2,
                    0,
                )
                ** 2
                for i in (0, 2)
            )
            if distance <= 300**2:
                raise ValueError("spawn intersects box")
        for other in spawns[:index]:
            if (
                sum((foot[i] - other["foot_mm"][i]) ** 2 for i in (0, 2))
                <= 600**2
            ):
                raise ValueError("spawns overlap")
    return scene


def encode(source: bytes) -> bytes:
    """Reads an OpenUSD source and encodes its validated primitive scene.

    Args:
        source: Exact bytes of a self-contained OpenUSD scenario.

    Returns:
        The canonical 152-byte scene payload.

    Raises:
        ValueError: The source or its scenario geometry is invalid.
        OSError: Temporary source storage fails.
    """
    scene = validate(usd_source.read(source))
    values = (
        scene["interior_mm"] + scene["wall_mm"] + scene["capsule_mm"] + [2, 4]
    )
    result = struct.pack("<8I", *values)
    for box in scene["boxes"]:
        result += struct.pack(
            "<I3i3I", box["id"], *box["center_mm"], *box["size_mm"]
        )
    for spawn in scene["spawns"]:
        result += struct.pack("<I3i", spawn["id"], *spawn["foot_mm"])
    return result


def decode(payload: bytes) -> SceneData:
    """Decodes and validates a primitive scene payload.

    Args:
        payload: Canonical scene bytes from a role pack.

    Returns:
        The validated scene data.

    Raises:
        ValueError: The payload length, counts or geometry is invalid.
    """
    if len(payload) != 152:
        raise ValueError("invalid scene length")
    (
        interior_width,
        interior_depth,
        wall_height,
        wall_thickness,
        capsule_height,
        capsule_diameter,
        box_count,
        spawn_count,
    ) = struct.unpack_from("<8I", payload)
    if (box_count, spawn_count) != (2, 4):
        raise ValueError("invalid primitive counts")
    scene: SceneData = {
        "schema": 1,
        "scenario": "mvp",
        "interior_mm": [interior_width, interior_depth],
        "wall_mm": [wall_height, wall_thickness],
        "capsule_mm": [capsule_height, capsule_diameter],
        "boxes": [],
        "spawns": [],
    }
    for offset in (32, 60):
        identity, *values = struct.unpack_from("<I3i3I", payload, offset)
        scene["boxes"].append(
            {"id": identity, "center_mm": values[:3], "size_mm": values[3:]}
        )
    for offset in (88, 104, 120, 136):
        identity, *foot = struct.unpack_from("<I3i", payload, offset)
        scene["spawns"].append({"id": identity, "foot_mm": foot})
    return validate(scene)
