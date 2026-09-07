"""Read the self-contained OpenUSD primitive scenario authoring profile."""

import math
import pathlib
import tempfile
from typing import TYPE_CHECKING

from pxr import Gf
from pxr import Sdf
from pxr import Tf
from pxr import Usd
from pxr import UsdGeom

if TYPE_CHECKING:
    from blackflower_cooker import scene


def read(source: bytes) -> "scene.SceneData":
    """Reads the self-contained primitive profile from exact source bytes.

    Args:
        source: Text or binary OpenUSD layer bytes.

    Returns:
        Scenario data in millimetres, with boxes and spawns sorted by identity.

    Raises:
        ValueError: The layer cannot be parsed or violates the source profile.
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


def _read_stage(stage) -> "scene.SceneData":
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

    def mm(value):
        distance = float(value) * units * 1000
        if (
            not math.isfinite(distance)
            or abs(distance) > 20000
            or abs(distance - round(distance)) > 0.001
        ):
            raise ValueError(
                "USD dimensions must resolve to millimetres within 0.001 mm"
            )
        return round(distance)

    root = stage.GetPrimAtPath("/Scenario")
    capsule = UsdGeom.Capsule(stage.GetPrimAtPath("/Scenario/Participant"))
    blocks = stage.GetPrimAtPath("/Scenario/Blocks")
    spawns = stage.GetPrimAtPath("/Scenario/Spawns")
    if (
        not root
        or not capsule
        or not blocks
        or not spawns
        or stage.GetDefaultPrim() != root
    ):
        raise ValueError("missing required USD scenario prims")
    if (
        root.GetTypeName() != "Xform"
        or blocks.GetTypeName() != "Scope"
        or spawns.GetTypeName() != "Scope"
    ):
        raise ValueError("invalid USD scenario container types")
    allowed = {
        root.GetPath(),
        capsule.GetPath(),
        blocks.GetPath(),
        spawns.GetPath(),
    }
    allowed.update(p.GetPath() for p in blocks.GetChildren())
    allowed.update(p.GetPath() for p in spawns.GetChildren())
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
    if (
        capsule.GetAxisAttr().Get() != UsdGeom.Tokens.y
        or capsule.ComputeLocalToWorldTransform(Usd.TimeCode.Default())
        != Gf.Matrix4d(1)
    ):
        raise ValueError(
            "participant capsule must be untransformed and Y aligned"
        )

    def attr(prim, name):
        value = prim.GetAttribute(name).Get()
        if value is None:
            raise ValueError(
                f"missing USD attribute {name} at {prim.GetPath()}"
            )
        return value

    radius, height = (
        capsule.GetRadiusAttr().Get(),
        capsule.GetHeightAttr().Get(),
    )
    result: "scene.SceneData" = {
        "schema": attr(root, "blackflower:schema"),
        "scenario": attr(root, "blackflower:scenario"),
        "interior_mm": [mm(v) for v in attr(root, "blackflower:interior")],
        "wall_mm": [mm(v) for v in attr(root, "blackflower:wall")],
        "capsule_mm": [mm(height + 2 * radius), mm(2 * radius)],
        "boxes": [],
        "spawns": [],
    }
    for prim in blocks.GetChildren():
        cube = UsdGeom.Cube(prim)
        if not cube:
            raise ValueError("static blocks must be USD cubes")
        matrix = cube.ComputeLocalToWorldTransform(Usd.TimeCode.Default())
        if any(
            not math.isfinite(matrix[i][j]) for i in range(4) for j in range(4)
        ):
            raise ValueError("nonfinite USD transform")
        if (
            any(
                abs(matrix[i][j]) > 1e-12
                for i in range(3)
                for j in range(4)
                if i != j
            )
            or any(matrix[i][i] <= 0 for i in range(3))
            or matrix[3][3] != 1
        ):
            raise ValueError(
                "blocks require positive axis-aligned USD transforms"
            )
        result["boxes"].append(
            {
                "id": attr(prim, "blackflower:id"),
                "center_mm": [mm(matrix[3][i]) for i in range(3)],
                "size_mm": [
                    mm(cube.GetSizeAttr().Get() * matrix[i][i])
                    for i in range(3)
                ],
            }
        )
    for prim in spawns.GetChildren():
        if prim.GetTypeName() != "Xform":
            raise ValueError("spawns must be USD Xforms")
        matrix = UsdGeom.Xformable(prim).ComputeLocalToWorldTransform(
            Usd.TimeCode.Default()
        )
        translation = matrix.ExtractTranslation()
        if matrix != Gf.Matrix4d(1).SetTranslate(translation):
            raise ValueError("spawns require translation-only transforms")
        result["spawns"].append(
            {
                "id": attr(prim, "blackflower:id"),
                "foot_mm": [mm(v) for v in translation],
            }
        )
    for key in ("boxes", "spawns"):
        result[key].sort(key=lambda value: value["id"])
    return result
