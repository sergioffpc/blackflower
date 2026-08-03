# SPDX-License-Identifier: GPL-2.0-or-later

"""Pure validation and serialization for Blackflower glTF metadata."""

from __future__ import annotations

from collections.abc import Iterable, Mapping
import math
import re
import struct
import unicodedata


ANIMATION_SCHEMA = 1
MAP_SCHEMA = 1
MAX_MARKERS = 4_096
MAX_MARKER_NAME_BYTES = 128
MAX_NODE_ID_BYTES = 128
MAX_ASSET_ID_BYTES = 255

_PORTABLE_KEY = re.compile(r"^[a-z][a-z0-9_]*$")
_MAP_ROLES = {
    "geometry",
    "spawn_point",
    "prefab_instance",
    "volume_instance",
    "trigger_volume",
    "navigation_anchor",
    "navigation_link",
    "acoustic_zone",
    "acoustic_portal",
    "audio_emitter",
}


class MetadataError(ValueError):
    """Authored metadata cannot be represented by a Blackflower schema."""


def build_animation_metadata(
    markers: Iterable[tuple[str, float]],
    *,
    frame_start: float,
    frame_end: float,
    frames_per_second: float,
    time_origin_frame: float,
    looping: bool = False,
    additive_enabled: bool = False,
    additive_reference: str = "animation",
    root_motion_enabled: bool = False,
    root_motion_joint: str = "",
    translation_axes: Iterable[str] = ("x", "z"),
    rotation_axes: Iterable[str] = ("y",),
    root_motion_reference: str = "skeleton",
    remove_from_pose: bool = True,
    loop_correction: bool = False,
) -> dict[str, object]:
    """Build schema-1 animation metadata from action-local marker frames.

    ``frame_start`` and ``frame_end`` are the effective exported range.
    ``time_origin_frame`` is the frame that the glTF exporter maps to zero.
    When the exporter does not slide timestamps it must be zero.
    """

    start = _finite_number(frame_start, "animation start frame")
    end = _finite_number(frame_end, "animation end frame")
    fps = _finite_number(frames_per_second, "frames per second")
    origin = _finite_number(time_origin_frame, "animation time origin")
    if end < start:
        raise MetadataError("animation end frame precedes its start frame")
    if fps <= 0.0:
        raise MetadataError("frames per second must be greater than zero")

    source = list(markers)
    if len(source) > MAX_MARKERS:
        raise MetadataError(
            f"animation declares {len(source)} markers; the limit is {MAX_MARKERS}"
        )
    encoded: list[tuple[dict[str, object], int]] = []
    seen: set[tuple[str, int]] = set()
    for index, marker in enumerate(source):
        try:
            name, frame = marker
        except (TypeError, ValueError) as error:
            raise MetadataError(
                f"animation marker {index} must contain a name and frame"
            ) from error

        _validate_marker_name(name, index)
        marker_frame = _finite_number(frame, f"animation marker {index} frame")
        if marker_frame < start or marker_frame > end:
            raise MetadataError(
                f"animation marker {index} `{name}` at frame {marker_frame:g} "
                f"is outside the exported range {start:g}..{end:g}"
            )

        seconds = _float32((marker_frame - origin) / fps)
        if not math.isfinite(seconds) or seconds < 0.0:
            raise MetadataError(
                f"animation marker {index} `{name}` maps to invalid glTF time"
            )
        bits = _float32_bits(seconds)
        key = (name, bits)
        if key in seen:
            raise MetadataError(
                f"animation duplicates marker `{name}` at {seconds:g} seconds"
            )
        seen.add(key)
        encoded.append(({"name": name, "time_seconds": seconds}, bits))

    # Python's stable sort keeps source order for markers at the same time.
    encoded.sort(key=lambda item: item[1])
    if additive_reference not in {"animation", "skeleton"}:
        raise MetadataError("additive reference must be animation or skeleton")
    if root_motion_reference not in {"absolute", "skeleton", "animation"}:
        raise MetadataError(
            "root motion reference must be absolute, skeleton, or animation"
        )
    translation = _validate_axes(translation_axes, "root translation axes")
    rotation = _validate_axes(rotation_axes, "root rotation axes")
    if root_motion_enabled:
        _validate_text(
            root_motion_joint,
            "root motion joint",
            MAX_MARKER_NAME_BYTES,
        )
        if not translation and not rotation:
            raise MetadataError(
                "root motion must extract at least one translation or rotation axis"
            )

    return {
        "schema": ANIMATION_SCHEMA,
        "loop": bool(looping),
        "additive": {
            "enabled": bool(additive_enabled),
            "reference": additive_reference,
        },
        "root_motion": {
            "enabled": bool(root_motion_enabled),
            "joint": root_motion_joint,
            "translation_axes": translation,
            "rotation_axes": rotation,
            "reference": root_motion_reference,
            "remove_from_pose": bool(remove_from_pose),
            "loop_correction": bool(loop_correction),
        },
        "markers": [item[0] for item in encoded],
    }


def build_map_node_metadata(
    role: str,
    identifier: str,
    *,
    render: bool = False,
    collision: bool = False,
    navigation: str = "none",
    acoustic_class: str = "ignored",
    spawn_set: str = "default",
    spawn_weight: float = 1.0,
    asset: str = "",
    definition: str = "",
    navigation_end: str = "",
    navigation_area: str = "",
    navigation_direction: str = "bidirectional",
    navigation_radius: float = 0.5,
    acoustic_zone_kind: str = "bounds",
    acoustic_zone: str = "",
    acoustic_zone_a: str = "",
    acoustic_zone_b: str = "",
    acoustic_controller: str = "",
    acoustic_initially_open: bool = True,
    sound: str = "",
    autoplay: bool = False,
) -> dict[str, object]:
    """Build strict schema-1 metadata for one authored map node."""

    if role not in _MAP_ROLES:
        raise MetadataError("map node role is not supported")
    _validate_portable_key(identifier, "map node id", MAX_NODE_ID_BYTES)
    result: dict[str, object] = {
        "schema": MAP_SCHEMA,
        "node": {"id": identifier, "role": role},
    }

    if role == "geometry":
        if navigation not in {"none", "surface", "obstacle"}:
            raise MetadataError("geometry navigation use is not supported")
        if acoustic_class not in {
            "ignored",
            "static",
            "dynamic_rigid",
            "dynamic_state",
        }:
            raise MetadataError("acoustic geometry class is not supported")
        if not render and not collision and navigation == "none" and acoustic_class == "ignored":
            raise MetadataError("geometry must enable at least one domain use")
        result["geometry"] = {
            "render": bool(render),
            "collision": bool(collision),
            "navigation": navigation,
            "acoustic_class": acoustic_class,
        }
    elif role == "spawn_point":
        _validate_portable_key(spawn_set, "spawn set", 64)
        weight = _finite_number(spawn_weight, "spawn weight")
        if weight <= 0.0:
            raise MetadataError("spawn weight must be greater than zero")
        result["spawn_point"] = {
            "set": spawn_set,
            "weight": _float32(weight),
        }
    elif role in {"prefab_instance", "volume_instance"}:
        _validate_asset_id(asset, f"{role} asset")
        result[role] = {"asset": asset}
    elif role == "trigger_volume":
        _validate_asset_id(definition, "trigger definition")
        result[role] = {"definition": definition}
    elif role == "navigation_anchor":
        result[role] = {}
    elif role == "navigation_link":
        _validate_portable_key(navigation_end, "navigation end node", MAX_NODE_ID_BYTES)
        _validate_portable_key(navigation_area, "navigation area", 64)
        if navigation_direction not in {"one_way", "bidirectional"}:
            raise MetadataError("navigation link direction is not supported")
        radius = _finite_number(navigation_radius, "navigation link radius")
        if radius <= 0.0:
            raise MetadataError("navigation link radius must be greater than zero")
        result[role] = {
            "end": navigation_end,
            "area": navigation_area,
            "direction": navigation_direction,
            "radius": _float32(radius),
        }
    elif role == "acoustic_zone":
        if acoustic_zone_kind not in {"identity", "bounds", "probes"}:
            raise MetadataError("acoustic zone kind is not supported")
        zone: dict[str, object] = {"kind": acoustic_zone_kind}
        if acoustic_zone_kind == "probes":
            _validate_portable_key(acoustic_zone, "probe acoustic zone", MAX_NODE_ID_BYTES)
            zone["zone"] = acoustic_zone
        elif acoustic_zone:
            raise MetadataError("only acoustic probe zones reference another zone")
        result[role] = zone
    elif role == "acoustic_portal":
        _validate_portable_key(acoustic_zone_a, "portal zone A", MAX_NODE_ID_BYTES)
        _validate_portable_key(acoustic_zone_b, "portal zone B", MAX_NODE_ID_BYTES)
        if acoustic_zone_a == acoustic_zone_b:
            raise MetadataError("portal zones must differ")
        portal: dict[str, object] = {
            "zone_a": acoustic_zone_a,
            "zone_b": acoustic_zone_b,
            "initially_open": bool(acoustic_initially_open),
        }
        if acoustic_controller:
            _validate_portable_key(
                acoustic_controller,
                "acoustic portal controller",
                MAX_NODE_ID_BYTES,
            )
            portal["controller"] = acoustic_controller
        result[role] = portal
    elif role == "audio_emitter":
        _validate_asset_id(sound, "audio emitter sound")
        result[role] = {"sound": sound, "autoplay": bool(autoplay)}

    return result


def build_map_material_metadata(
    *,
    physics_material: str = "",
    navigation_area: str = "",
    acoustic_material: str = "",
) -> dict[str, object] | None:
    """Build schema-1 map surface metadata for one glTF material."""

    if not physics_material and not navigation_area and not acoustic_material:
        return None
    material: dict[str, object] = {}
    if physics_material:
        _validate_asset_id(physics_material, "physics material")
        material["physics_material"] = physics_material
    if navigation_area:
        _validate_portable_key(navigation_area, "navigation area", 64)
        material["navigation_area"] = navigation_area
    if acoustic_material:
        _validate_asset_id(acoustic_material, "acoustic material")
        material["acoustic_material"] = acoustic_material
    return {
        "schema": MAP_SCHEMA,
        "material": material,
    }


def validate_map_references(nodes: Iterable[Mapping[str, object]]) -> None:
    """Validate IDs and cross-node references in one exported map scene."""

    indexed: dict[str, Mapping[str, object]] = {}
    for metadata in nodes:
        node = metadata.get("node")
        if not isinstance(node, Mapping):
            raise MetadataError("map node metadata is missing its node identity")
        identifier = node.get("id")
        role = node.get("role")
        if not isinstance(identifier, str) or not isinstance(role, str):
            raise MetadataError("map node identity is invalid")
        if identifier in indexed:
            raise MetadataError(f"map duplicates node id `{identifier}`")
        indexed[identifier] = metadata

    for identifier, metadata in indexed.items():
        node = metadata["node"]
        assert isinstance(node, Mapping)
        role = node["role"]
        if role == "navigation_link":
            payload = metadata["navigation_link"]
            assert isinstance(payload, Mapping)
            _require_role(indexed, payload["end"], {"navigation_anchor"}, identifier)
        elif role == "acoustic_zone":
            payload = metadata["acoustic_zone"]
            assert isinstance(payload, Mapping)
            if payload["kind"] == "probes":
                _require_acoustic_zone(indexed, payload["zone"], "identity", identifier)
        elif role == "acoustic_portal":
            payload = metadata["acoustic_portal"]
            assert isinstance(payload, Mapping)
            _require_acoustic_zone(indexed, payload["zone_a"], "bounds", identifier)
            _require_acoustic_zone(indexed, payload["zone_b"], "bounds", identifier)
            controller = payload.get("controller")
            if controller is not None:
                _require_role(
                    indexed,
                    controller,
                    {"geometry", "prefab_instance"},
                    identifier,
                )


def _require_role(
    indexed: Mapping[str, Mapping[str, object]],
    target: object,
    expected: set[str],
    owner: str,
) -> Mapping[str, object]:
    if not isinstance(target, str) or target not in indexed:
        raise MetadataError(f"map node `{owner}` references missing node `{target}`")
    metadata = indexed[target]
    node = metadata["node"]
    assert isinstance(node, Mapping)
    role = node["role"]
    if role not in expected:
        choices = ", ".join(sorted(expected))
        raise MetadataError(
            f"map node `{owner}` references `{target}` with role `{role}`; "
            f"expected {choices}"
        )
    return metadata


def _require_acoustic_zone(
    indexed: Mapping[str, Mapping[str, object]],
    target: object,
    expected_kind: str,
    owner: str,
) -> None:
    metadata = _require_role(indexed, target, {"acoustic_zone"}, owner)
    payload = metadata["acoustic_zone"]
    assert isinstance(payload, Mapping)
    if payload["kind"] != expected_kind:
        raise MetadataError(
            f"map node `{owner}` references acoustic zone `{target}` with kind "
            f"`{payload['kind']}`; expected `{expected_kind}`"
        )


def merge_extras(
    extras: Mapping[str, object] | None,
    blackflower: Mapping[str, object] | None,
) -> dict[str, object] | None:
    """Preserve third-party extras and add exactly one owned namespace."""

    if blackflower is None:
        return dict(extras) if extras is not None else None
    if extras is not None and not isinstance(extras, Mapping):
        raise MetadataError("existing glTF extras must be an object")

    merged = dict(extras or {})
    if "blackflower" in merged:
        raise MetadataError(
            "glTF extras already contain `blackflower`; remove the conflicting "
            "custom property or disable the Blackflower exporter"
        )
    merged["blackflower"] = dict(blackflower)
    return merged


def _validate_marker_name(name: object, index: int) -> None:
    if not isinstance(name, str):
        raise MetadataError(f"animation marker {index} name must be text")
    _validate_text(name, f"animation marker {index} name", MAX_MARKER_NAME_BYTES)


def _validate_axes(values: Iterable[str], field: str) -> list[str]:
    axes = list(values)
    if any(axis not in {"x", "y", "z"} for axis in axes):
        raise MetadataError(f"{field} may contain only x, y, and z")
    if len(set(axes)) != len(axes):
        raise MetadataError(f"{field} cannot contain duplicates")
    return axes


def _validate_text(value: str, field: str, maximum_bytes: int) -> None:
    if (
        not value
        or value.strip() != value
        or len(value.encode("utf-8")) > maximum_bytes
        or any(unicodedata.category(character) == "Cc" for character in value)
    ):
        raise MetadataError(
            f"{field} must be non-empty, unpadded, free of control characters, "
            f"and at most {maximum_bytes} UTF-8 bytes"
        )


def _validate_portable_key(value: str, field: str, maximum_bytes: int) -> None:
    if (
        not isinstance(value, str)
        or len(value.encode("utf-8")) > maximum_bytes
        or _PORTABLE_KEY.fullmatch(value) is None
    ):
        raise MetadataError(
            f"{field} must be lower_snake_case and at most {maximum_bytes} UTF-8 bytes"
        )


def _validate_asset_id(value: str, field: str) -> None:
    if (
        not isinstance(value, str)
        or not value
        or len(value.encode("utf-8")) > MAX_ASSET_ID_BYTES
        or not value.isascii()
        or any(
            not segment
            or segment in {".", ".."}
            or any(
                not (
                    character.islower()
                    or character.isdigit()
                    or character in "._-"
                )
                for character in segment
            )
            for segment in value.split("/")
        )
    ):
        raise MetadataError(
            f"{field} must be a portable lowercase asset ID of at most 255 bytes"
        )


def _finite_number(value: object, field: str) -> float:
    if isinstance(value, bool):
        raise MetadataError(f"{field} must be a finite number")
    try:
        number = float(value)
    except (TypeError, ValueError, OverflowError) as error:
        raise MetadataError(f"{field} must be a finite number") from error
    if not math.isfinite(number):
        raise MetadataError(f"{field} must be a finite number")
    return number


def _float32(value: float) -> float:
    try:
        return struct.unpack("<f", struct.pack("<f", value))[0]
    except OverflowError as error:
        raise MetadataError("marker time does not fit a glTF float") from error


def _float32_bits(value: float) -> int:
    return struct.unpack("<I", struct.pack("<f", value))[0]
