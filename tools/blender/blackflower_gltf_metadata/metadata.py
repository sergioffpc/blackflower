# SPDX-License-Identifier: GPL-2.0-or-later

"""Pure validation and serialization for Blackflower glTF metadata."""

from __future__ import annotations

from collections.abc import Iterable, Mapping
import math
import re
import struct
import unicodedata


ANIMATION_SCHEMA = 1
NODE_SCHEMA = 1
MAX_MARKERS = 4_096
MAX_MARKER_NAME_BYTES = 128
MAX_NODE_KIND_BYTES = 64
MAX_NODE_ID_BYTES = 128

_NODE_KIND = re.compile(r"^[a-z][a-z0-9_]*$")


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


def build_node_metadata(
    kind: str,
    identifier: str = "",
    *,
    navigation_role: str = "none",
    area_key: str = "",
    direction: str = "bidirectional",
    radius: float = 0.0,
    acoustics_kind: str = "none",
    geometry_class: str = "static",
    acoustic_zone: str = "",
) -> dict[str, object]:
    """Build schema-1 typed node metadata for level cooking."""

    if not isinstance(kind, str):
        raise MetadataError("node kind must be text")
    if (
        not kind
        or len(kind.encode("utf-8")) > MAX_NODE_KIND_BYTES
        or _NODE_KIND.fullmatch(kind) is None
    ):
        raise MetadataError(
            "node kind must be lower_snake_case and at most "
            f"{MAX_NODE_KIND_BYTES} UTF-8 bytes"
        )

    node: dict[str, str] = {"kind": kind}
    if identifier:
        _validate_text(identifier, "node id", MAX_NODE_ID_BYTES)
        node["id"] = identifier
    result: dict[str, object] = {"schema": NODE_SCHEMA, "node": node}
    if navigation_role != "none":
        if not identifier:
            raise MetadataError("navigation node id is required")
        if navigation_role not in {"surface", "obstacle", "off_mesh_link"}:
            raise MetadataError("navigation role is not supported")

        navigation: dict[str, object] = {"role": navigation_role}
        if navigation_role in {"surface", "off_mesh_link"}:
            _validate_portable_key(area_key, "navigation area key")
            navigation["area_key"] = area_key
        elif area_key:
            raise MetadataError("navigation obstacle cannot declare an area key")
        if navigation_role == "off_mesh_link":
            if direction not in {"one_way", "bidirectional"}:
                raise MetadataError("off-mesh link direction is not supported")
            link_radius = _finite_number(radius, "off-mesh link radius")
            if link_radius <= 0.0:
                raise MetadataError("off-mesh link radius must be greater than zero")
            navigation["direction"] = direction
            navigation["radius"] = _float32(link_radius)
        result["navigation"] = navigation

    if acoustics_kind != "none":
        if not identifier:
            raise MetadataError("acoustic node id is required")
        expected_kinds = {
            "geometry": "acoustic_geometry",
            "zone": "acoustic_zone",
            "probe_volume": "acoustic_probe_volume",
        }
        expected_kind = expected_kinds.get(acoustics_kind)
        if expected_kind is None:
            raise MetadataError("acoustic node kind is not supported")
        if kind != expected_kind:
            raise MetadataError(
                f"{acoustics_kind} acoustics requires node kind {expected_kind}"
            )
        acoustics: dict[str, object] = {"kind": acoustics_kind}
        if acoustics_kind == "geometry":
            if geometry_class not in {
                "static",
                "dynamic_rigid",
                "dynamic_state",
                "ignored",
            }:
                raise MetadataError("acoustic geometry class is not supported")
            acoustics["class"] = geometry_class
        elif acoustics_kind == "probe_volume":
            _validate_text(acoustic_zone, "acoustic zone id", MAX_NODE_ID_BYTES)
            acoustics["zone"] = acoustic_zone
        result["acoustics"] = acoustics

    return result


def build_material_metadata(material: str) -> dict[str, object] | None:
    """Build schema-1 acoustic metadata for one glTF material."""

    if not material:
        return None
    _validate_asset_id(material, "acoustic material")
    return {
        "schema": NODE_SCHEMA,
        "acoustics": {"material": material},
    }


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


def _validate_portable_key(value: str, field: str) -> None:
    if (
        not isinstance(value, str)
        or len(value.encode("utf-8")) > 64
        or _NODE_KIND.fullmatch(value) is None
    ):
        raise MetadataError(
            f"{field} must be lower_snake_case and at most 64 UTF-8 bytes"
        )


def _validate_asset_id(value: str, field: str) -> None:
    if (
        not isinstance(value, str)
        or not value
        or len(value.encode("utf-8")) > 255
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
