"""Read the self-contained OpenUSD primitive scenario authoring subset."""

from collections.abc import Callable
import math
import pathlib
import tempfile
from typing import Any
from typing import cast

from pxr import Gf
from pxr import Sdf
from pxr import Tf
from pxr import Usd
from pxr import UsdGeom

from content import scene


def read(source: bytes) -> "scene.SceneData":
    """Reads the self-contained primitive subset from exact source bytes.

    Args:
        source: Text or binary OpenUSD layer bytes.

    Returns:
        Scenario data in millimetres, with collections sorted by identity.

    Raises:
        ValueError: The layer cannot be parsed or violates the source contract.
        OSError: Temporary source storage fails.
    """
    # Parse precisely the bytes whose hash is recorded, with no ambient asset
    # resolver or layer cache able to substitute a different source revision.
    with tempfile.TemporaryDirectory(prefix="blackflower-usd-") as directory:
        path = pathlib.Path(directory) / "scene.usd"
        path.write_bytes(source)
        try:
            stage = Usd.Stage.Open(str(path), load=Usd.Stage.LoadNone)
            return _read_stage(stage)
        except Tf.ErrorException as error:
            raise ValueError(f"invalid OpenUSD scene: {error}") from error


def _children(prim: Usd.Prim) -> list[Usd.Prim]:
    # types-usd omits the element type of the USD child-prim collection.
    return cast(
        list[Usd.Prim],
        prim.GetChildren(),  # pyright: ignore[reportUnknownMemberType]
    )


def _read_stage(stage: Usd.Stage) -> "scene.SceneData":
    layer = stage.GetRootLayer()
    if layer.subLayerPaths or len(stage.GetUsedLayers()) != 2:
        raise ValueError("primitive-v1 requires a self-contained USD layer")
    if (
        not stage.HasAuthoredMetadata("metersPerUnit")
        or UsdGeom.GetStageUpAxis(stage) != UsdGeom.Tokens.y
    ):
        raise ValueError("USD stage requires explicit metersPerUnit and Y up")
    units = UsdGeom.GetStageMetersPerUnit(stage)
    if not math.isfinite(units) or units <= 0:
        raise ValueError("invalid USD metres per unit")

    def mm(value: float) -> int:
        distance = float(value) * units * 1000
        if (
            not math.isfinite(distance)
            or abs(distance - round(distance)) > 0.001
        ):
            raise ValueError(
                "USD dimensions must resolve to millimetres within 0.001 mm"
            )
        return round(distance)

    root = stage.GetPrimAtPath("/Scenario")
    blocks = stage.GetPrimAtPath("/Scenario/CollisionShapes")
    lights = stage.GetPrimAtPath("/Scenario/Lights")
    spawns = stage.GetPrimAtPath("/Scenario/Spawns")
    if (
        not root
        or not lights
        or not blocks
        or not spawns
        or stage.GetDefaultPrim() != root
    ):
        raise ValueError("missing required USD scenario prims")
    if (
        root.GetTypeName() != "Xform"
        or blocks.GetTypeName() != "Scope"
        or spawns.GetTypeName() != "Scope"
        or lights.GetTypeName() != "Scope"
    ):
        raise ValueError("invalid USD scenario container types")
    allowed = {
        root.GetPath(),
        lights.GetPath(),
        blocks.GetPath(),
        spawns.GetPath(),
    }
    allowed.update(p.GetPath() for p in _children(lights))
    allowed.update(p.GetPath() for p in _children(blocks))
    allowed.update(p.GetPath() for p in _children(spawns))
    for prim in stage.TraverseAll():
        if (
            prim.GetPath() not in allowed
            or not prim.IsActive()
            or prim.IsInstance()
            or prim.HasAuthoredReferences()
            or prim.HasAuthoredPayloads()
            or prim.HasAuthoredInherits()
            or prim.HasAuthoredSpecializes()
            or prim.GetVariantSets().GetNames()
            or prim.GetAuthoredRelationships()
        ):
            raise ValueError(
                f"unsupported USD prim or composition: {prim.GetPath()}"
            )
        for attribute in prim.GetAuthoredAttributes():
            if attribute.HasAuthoredConnections():
                raise ValueError("unsupported USD attribute connection")
            if attribute.GetNumTimeSamples() or attribute.GetTypeName() in (
                Sdf.ValueTypeNames.Asset,
                Sdf.ValueTypeNames.AssetArray,
            ):
                raise ValueError(
                    "animation and asset dependencies are outside primitive-v1"
                )
    if UsdGeom.Xformable(root).ComputeLocalToWorldTransform(
        Usd.TimeCode.Default()
    ) != Gf.Matrix4d(1):
        raise ValueError("Scenario transform must be identity")

    def attr(prim: Usd.Prim, name: str) -> Any:
        value = prim.GetAttribute(name).Get()
        if value is None:
            raise ValueError(
                f"missing USD attribute {name} at {prim.GetPath()}"
            )
        return value

    if attr(root, "blackflower:schema") != 1:
        raise ValueError("unsupported scene schema")
    result: "scene.SceneData" = {
        "collision_shapes": [],
        "lights": [],
        "spawns": [],
    }
    for prim in _children(blocks):
        if prim.GetTypeName() not in ("Cube", "Sphere"):
            raise ValueError("unsupported collision shape kind")
        matrix = UsdGeom.Xformable(prim).ComputeLocalToWorldTransform(
            Usd.TimeCode.Default()
        )
        if any(
            not math.isfinite(matrix[i, j]) for i in range(4) for j in range(4)
        ):
            raise ValueError("nonfinite USD transform")
        if (
            any(
                abs(matrix[i, j]) > 1e-12
                for i in range(3)
                for j in range(4)
                if i != j
            )
            or any(matrix[i, i] <= 0 for i in range(3))
            or matrix[3, 3] != 1
        ):
            raise ValueError(
                "blocks require positive axis-aligned USD transforms"
            )
        if prim.GetTypeName() == "Cube":
            kind = scene.CollisionShapeKind.BOX
            dimensions = [
                mm(attr(prim, "size") * matrix[i, i]) for i in range(3)
            ]
        else:
            if matrix[0, 0] != matrix[1, 1] or matrix[1, 1] != matrix[2, 2]:
                raise ValueError("spheres require uniform scale")
            kind = scene.CollisionShapeKind.SPHERE
            dimensions = [mm(attr(prim, "radius") * matrix[0, 0])]
        result["collision_shapes"].append(
            {
                "id": attr(prim, "blackflower:id"),
                "kind": kind,
                "center_mm": [mm(matrix[3, i]) for i in range(3)],
                "dimensions_mm": dimensions,
            }
        )
    for prim in _children(lights):
        light_kind = attr(prim, "blackflower:lightType")
        position = _position(prim, mm)
        if light_kind == "point":
            result["lights"].append(
                {
                    "kind": scene.LightKind.POINT,
                    "id": attr(prim, "blackflower:id"),
                    "position_mm": position,
                    "color": list(attr(prim, "blackflower:color")),
                    "intensity": attr(prim, "blackflower:intensity"),
                }
            )
        elif light_kind == "directional":
            result["lights"].append(
                {
                    "kind": scene.LightKind.DIRECTIONAL,
                    "id": attr(prim, "blackflower:id"),
                    "direction": list(attr(prim, "blackflower:direction")),
                    "color": list(attr(prim, "blackflower:color")),
                    "intensity": attr(prim, "blackflower:intensity"),
                }
            )
        else:
            raise ValueError("unsupported light kind")
    for prim in _children(spawns):
        result["spawns"].append(
            {
                "id": attr(prim, "blackflower:id"),
                "position_mm": _position(prim, mm),
            }
        )
    for key in ("collision_shapes", "lights", "spawns"):
        result[key].sort(key=lambda value: value["id"])
    return result


def _position(prim: Usd.Prim, mm: Callable[[float], int]) -> list[int]:
    if prim.GetTypeName() != "Xform":
        raise ValueError("placements must be USD Xforms")
    matrix = UsdGeom.Xformable(prim).ComputeLocalToWorldTransform(
        Usd.TimeCode.Default()
    )
    translation = matrix.ExtractTranslation()
    if matrix != Gf.Matrix4d(1).SetTranslate(translation):
        raise ValueError("placements require translation-only transforms")
    return [mm(matrix[3, axis]) for axis in range(3)]
