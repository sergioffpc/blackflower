"""Encoding of scene entities and their independently authored colliders."""

import enum
import math
import re
import struct
from typing import TypedDict


class SceneRole(enum.Enum):
    """Consumer role selecting the concrete scene contract."""

    SERVER = "server"
    AGENT = "agent"
    CLIENT = "client"


class CollisionDomain(enum.IntEnum):
    """Collision participation supported by the current scene schema."""

    SESSION_STATIC = 1


COLLISION_COMPONENT = 1
VISUAL_COMPONENT = 2
AUDIO_COMPONENT = 4
ALL_COMPONENTS = COLLISION_COMPONENT | VISUAL_COMPONENT | AUDIO_COMPONENT


class ColliderBoxData(TypedDict):
    """Entity-local oriented box, with metre dimensions and XYZW rotation."""

    center_m: list[float]
    dimensions_m: list[float]
    rotation_xyzw: list[float]


class SceneEntityDescription(TypedDict):
    """Persistent identity, placement, and optional role domains.

    Attributes:
        id: Scene-local authored identity.
        position_m: Position in metres.
        rotation_xyzw: Unit quaternion in XYZW order.
        scale: Positive uniform placement scale.
        collision_domain: Collision authority, when colliders are present.
        colliders: Independently authored entity-local collision boxes.
        visual_ref: Logical visual asset reference for presentation.
        audio_ref: Logical audio asset reference for presentation.
    """

    id: str
    position_m: list[float]
    rotation_xyzw: list[float]
    scale: float
    collision_domain: CollisionDomain | None
    colliders: list[ColliderBoxData]
    visual_ref: str | None
    audio_ref: str | None


class SceneData(TypedDict):
    """A scene consists only of entities."""

    entities: list[SceneEntityDescription]


def project(data: SceneData, role: SceneRole) -> SceneData:
    """Projects source descriptions into one sparse consumer scene."""
    entities = []
    for entity in data["entities"]:
        projected = entity.copy()
        if role != SceneRole.CLIENT:
            projected["visual_ref"] = None
            projected["audio_ref"] = None
        if _component_mask(projected):
            entities.append(projected)
    return {"entities": entities}


def encode(data: SceneData, role: SceneRole) -> bytes:
    """Encodes one concrete role scene in scene v1 record order.

    Args:
        data: Projected entities sorted by their unique ASCII identities.
        role: Concrete role whose component domains are permitted.

    Returns:
        Entity count followed by entity records and their colliders.

    Raises:
        ValueError: A field cannot be represented by this schema.
    """
    try:
        result = bytearray(struct.pack("<I", len(data["entities"])))
        for entity in data["entities"]:
            result.extend(_encode_entity(entity, role))
    except (struct.error, OverflowError) as error:
        raise ValueError("scene field cannot be encoded") from error
    return bytes(result)


def _encode_entity(entity: SceneEntityDescription, role: SceneRole) -> bytes:
    identity = entity["id"].encode("ascii")
    mask = _component_mask(entity)
    _validate_components(entity, role, mask)
    result = bytearray(struct.pack("<I", len(identity)) + identity)
    result.extend(
        struct.pack(
            "<8dI",
            *entity["position_m"],
            *entity["rotation_xyzw"],
            entity["scale"],
            mask,
        )
    )
    if entity["collision_domain"] is not None:
        result.extend(
            struct.pack(
                "<2I",
                entity["collision_domain"],
                len(entity["colliders"]),
            )
        )
        for collider in entity["colliders"]:
            result.extend(_encode_collider(collider))
    for reference in (entity["visual_ref"], entity["audio_ref"]):
        if reference is not None:
            encoded = reference.encode("ascii")
            result.extend(struct.pack("<I", len(encoded)) + encoded)
    return bytes(result)


def _encode_collider(collider: ColliderBoxData) -> bytes:
    return struct.pack(
        "<I10d",
        1,
        *collider["center_m"],
        *collider["dimensions_m"],
        *collider["rotation_xyzw"],
    )


def decode(payload: bytes, role: SceneRole) -> SceneData:
    """Decodes complete validated entities with no partial result.

    Args:
        payload: Scene v1 encoded bytes.
        role: Concrete role whose component domains are permitted.

    Returns:
        Entities with their owned colliders, in encoded order.

    Raises:
        ValueError: Identity, geometry, record type or byte layout is invalid.
    """
    if len(payload) < 4:
        raise ValueError("invalid scene length")
    count = struct.unpack_from("<I", payload)[0]
    data: SceneData = {"entities": []}
    offset = 4
    for _ in range(count):
        entity, offset = _decode_scene_entity_description(payload, offset, role)
        if data["entities"] and entity["id"] <= data["entities"][-1]["id"]:
            raise ValueError("entity IDs must be unique and sorted")
        data["entities"].append(entity)
    if offset != len(payload):
        raise ValueError("invalid scene length")
    return data


def _decode_scene_entity_description(
    payload: bytes, offset: int, role: SceneRole
) -> tuple[SceneEntityDescription, int]:
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
    mask = values[8]
    entity: SceneEntityDescription = {
        "id": identity,
        "position_m": position,
        "rotation_xyzw": rotation,
        "scale": values[7],
        "collision_domain": None,
        "colliders": [],
        "visual_ref": None,
        "audio_ref": None,
    }
    offset = end + 68
    if mask & COLLISION_COMPONENT:
        entity, offset = _decode_collision(payload, offset, entity)
    if mask & VISUAL_COMPONENT:
        entity["visual_ref"], offset = _decode_reference(payload, offset)
    if mask & AUDIO_COMPONENT:
        entity["audio_ref"], offset = _decode_reference(payload, offset)
    _validate_components(entity, role, mask)
    return entity, offset


def _decode_collision(
    payload: bytes, offset: int, entity: SceneEntityDescription
) -> tuple[SceneEntityDescription, int]:
    if offset + 8 > len(payload):
        raise ValueError("invalid collision length")
    domain, count = struct.unpack_from("<2I", payload, offset)
    try:
        entity["collision_domain"] = CollisionDomain(domain)
    except ValueError as error:
        raise ValueError("unsupported collision domain") from error
    offset += 8
    for _ in range(count):
        collider, offset = _decode_collider(payload, offset)
        entity["colliders"].append(collider)
    return entity, offset


def _decode_reference(payload: bytes, offset: int) -> tuple[str, int]:
    if offset + 4 > len(payload):
        raise ValueError("invalid reference length")
    size = struct.unpack_from("<I", payload, offset)[0]
    start, end = offset + 4, offset + 4 + size
    if end > len(payload):
        raise ValueError("invalid reference length")
    reference = payload[start:end].decode("ascii")
    if not _valid_reference(reference):
        raise ValueError("invalid logical reference")
    return reference, end


def _decode_collider(
    payload: bytes, offset: int
) -> tuple[ColliderBoxData, int]:
    if offset + 4 > len(payload):
        raise ValueError("invalid collider length")
    if struct.unpack_from("<I", payload, offset)[0] != 1:
        raise ValueError("unsupported collider kind")
    if offset + 84 > len(payload):
        raise ValueError("invalid collider length")
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


def _component_mask(entity: SceneEntityDescription) -> int:
    mask = 0
    if entity["collision_domain"] is not None:
        mask |= COLLISION_COMPONENT
    if entity["visual_ref"] is not None:
        mask |= VISUAL_COMPONENT
    if entity["audio_ref"] is not None:
        mask |= AUDIO_COMPONENT
    return mask


def _validate_components(
    entity: SceneEntityDescription, role: SceneRole, mask: int
) -> None:
    if mask == 0 or mask & ~ALL_COMPONENTS:
        raise ValueError("invalid scene component mask")
    if role != SceneRole.CLIENT and mask != COLLISION_COMPONENT:
        raise ValueError("presentation component in headless scene")
    has_collision = entity["collision_domain"] is not None
    if has_collision != bool(entity["colliders"]):
        raise ValueError("collision domain requires colliders")
    for reference in (entity["visual_ref"], entity["audio_ref"]):
        if reference is not None and not _valid_reference(reference):
            raise ValueError("invalid logical reference")


def _valid_reference(reference: str) -> bool:
    return re.fullmatch(r"[A-Za-z0-9][A-Za-z0-9_.:-]*", reference) is not None
