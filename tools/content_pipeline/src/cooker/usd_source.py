"""Read referenced scene entities with independently authored collision."""

import math
import pathlib
import re
import tempfile
from typing import Any
from typing import cast

from pxr import Gf
from pxr import Sdf
from pxr import Tf
from pxr import Usd
from pxr import UsdGeom
from pxr import UsdPhysics

from cooker import scene
from cooker import usd_dependencies


def read(source: pathlib.Path) -> tuple[scene.SceneData, bytes]:
    """Reads a private snapshot of the scene and its entity definitions.

    Args:
        source: Scene layer with local relative entity references.

    Returns:
        Prepared scene values and the exact dependency provenance transcript.

    Raises:
        ValueError: USD or authored content violates the bounded contract.
        OSError: Reading or snapshotting a dependency fails.
    """
    with tempfile.TemporaryDirectory(prefix="blackflower-usd-") as directory:
        try:
            snapshot = usd_dependencies.capture(source, pathlib.Path(directory))
            stage = Usd.Stage.Open(str(snapshot.root), load=Usd.Stage.LoadNone)
            return _read_stage(stage), snapshot.transcript
        except Tf.ErrorException as error:
            raise ValueError(f"invalid OpenUSD scene: {error}") from error


def _children(prim: Usd.Prim) -> list[Usd.Prim]:
    if not prim:
        return []
    return cast(list[Usd.Prim], prim.GetChildren())


def _read_stage(stage: Usd.Stage) -> scene.SceneData:
    _validate_units(stage)
    root = stage.GetDefaultPrim()
    if root.GetPath() != Sdf.Path("/Scene") or root.GetTypeName() != "Xform":
        raise ValueError("USD requires a Scene default Xform")
    if _transform(root) != Gf.Matrix4d(1):
        raise ValueError("Scene transform must be identity")
    if _attribute(root, "blackflower:schema") != 1:
        raise ValueError("unsupported scene schema")
    entities = _scope(root, "Entities")
    data: scene.SceneData = {"entities": []}
    allowed = {root.GetPath()}
    if entities:
        allowed.add(entities.GetPath())
    for entity in sorted(_children(entities), key=_identity):
        _read_entity(entity, data, allowed)
    for prim in stage.TraverseAll():
        if prim.GetPath() not in allowed:
            raise ValueError(f"unsupported USD prim: {prim.GetPath()}")
        _validate_prim(prim)
    return data


def _validate_units(stage: Usd.Stage) -> None:
    if (
        not stage.HasAuthoredMetadata("metersPerUnit")
        or UsdGeom.GetStageMetersPerUnit(stage) != 1
        or UsdGeom.GetStageUpAxis(stage) != UsdGeom.Tokens.y
    ):
        raise ValueError("USD requires explicit metre units and Y up")


def _scope(parent: Usd.Prim, name: str) -> Usd.Prim:
    prim = parent.GetChild(name)
    if prim and prim.GetTypeName() != "Scope":
        raise ValueError(f"expected Scope at {prim.GetPath()}")
    return prim


def _identity(prim: Usd.Prim) -> str:
    identity = _attribute(prim, "blackflower:id")
    if not isinstance(identity, str) or not re.fullmatch(
        r"[A-Za-z0-9][A-Za-z0-9_.:-]*", identity
    ):
        raise ValueError("entity ID must match [A-Za-z0-9][A-Za-z0-9_.:-]*")
    return identity


def _read_entity(
    prim: Usd.Prim, data: scene.SceneData, allowed: set[Sdf.Path]
) -> None:
    identity = _identity(prim)
    if data["entities"] and data["entities"][-1]["id"] == identity:
        raise ValueError(f"duplicate entity ID: {identity}")
    if not prim.HasAuthoredReferences():
        raise ValueError("scene entities require a definition reference")
    matrix = _transform(prim, uniform=True)
    transform = Gf.Transform(matrix)
    data["entities"].append(
        {
            "id": identity,
            "position_m": _vector(matrix.ExtractTranslation()),
            "rotation_xyzw": _quaternion(transform.GetRotation().GetQuat()),
            "scale": _vector(transform.GetScale())[0],
            "collision_domain": None,
            "colliders": [],
            "visual_ref": _logical_reference(prim, "blackflower:visual"),
            "audio_ref": _logical_reference(prim, "blackflower:audio"),
        }
    )
    allowed.add(prim.GetPath())
    colliders, visuals = _scope(prim, "Bounds"), _scope(prim, "Visuals")
    allowed.update(p.GetPath() for p in (colliders, visuals) if p)
    if _children(visuals):
        raise ValueError("visual source import is not supported yet")
    for collider in sorted(
        _children(colliders), key=lambda p: str(p.GetName())
    ):
        allowed.add(collider.GetPath())
        data["entities"][-1]["colliders"].append(_box(collider))
    if data["entities"][-1]["colliders"]:
        data["entities"][-1][
            "collision_domain"
        ] = scene.CollisionDomain.SESSION_STATIC


def _logical_reference(prim: Usd.Prim, name: str) -> str | None:
    attribute = prim.GetAttribute(name)
    if not attribute:
        return None
    value = attribute.Get()
    if not isinstance(value, str) or not re.fullmatch(
        r"[A-Za-z0-9][A-Za-z0-9_.:-]*", value
    ):
        raise ValueError(f"{name} must be a logical asset reference")
    return value


def _box(prim: Usd.Prim) -> scene.ColliderBoxData:
    if prim.GetTypeName() != "Cube" or not prim.HasAPI("PhysicsCollisionAPI"):
        raise ValueError("colliders require Cube with PhysicsCollisionAPI")
    if not UsdPhysics.CollisionAPI(prim).GetCollisionEnabledAttr().Get():
        raise ValueError("disabled colliders are unsupported")
    matrix = _transform(prim)
    transform = Gf.Transform(matrix)
    scales = _vector(transform.GetScale())
    size = float(_attribute(prim, "size"))
    if not math.isfinite(size) or size <= 0:
        raise ValueError("collider size must be finite and positive")
    return {
        "center_m": _vector(matrix.ExtractTranslation()),
        "dimensions_m": [float(size * scale) for scale in scales],
        "rotation_xyzw": _quaternion(transform.GetRotation().GetQuat()),
    }


def _quaternion(value: Gf.Quatd) -> list[float]:
    value = value.GetNormalized()
    result = [*_vector(value.GetImaginary()), value.GetReal()]
    return [-v for v in result] if result[3] < 0 else result


def _vector(value: Gf.Vec3d) -> list[float]:
    # The pinned USD stubs incorrectly type scalar Vec3d indexing as a list.
    return [cast(float, value[i]) for i in range(3)]


def _transform(prim: Usd.Prim, *, uniform: bool = False) -> Gf.Matrix4d:
    if prim.GetTypeName() not in ("Xform", "Cube"):
        raise ValueError("unsupported transform prim type")
    xform = UsdGeom.Xformable(prim)
    names = [str(op.GetOpName()) for op in xform.GetOrderedXformOps()]
    allowed = ["xformOp:translate", "xformOp:rotateXYZ", "xformOp:scale"]
    authored = list(xform.GetXformOpOrderAttr().Get() or [])
    if (
        xform.GetResetXformStack()
        or names != authored
        or names != [n for n in allowed if n in names]
    ):
        raise ValueError("transforms require translate, rotateXYZ, scale order")
    for name in names:
        values = _attribute(prim, name)
        if not all(math.isfinite(v) for v in values):
            raise ValueError("nonfinite transform")
        if name == "xformOp:scale" and (
            any(v <= 0 for v in values)
            or (uniform and (values[0] != values[1] or values[1] != values[2]))
        ):
            raise ValueError("entity scale must be positive and uniform")
    return xform.GetLocalTransformation()


def _validate_prim(prim: Usd.Prim) -> None:
    if (
        not prim.IsActive()
        or prim.IsInstance()
        or prim.HasAuthoredPayloads()
        or prim.HasAuthoredInherits()
        or prim.HasAuthoredSpecializes()
        or prim.GetVariantSets().GetNames()
        or prim.GetAuthoredRelationships()
        or any(api != "PhysicsCollisionAPI" for api in prim.GetAppliedSchemas())
    ):
        raise ValueError(
            f"unsupported USD composition or physics: {prim.GetPath()}"
        )
    for attribute in prim.GetAuthoredAttributes():
        if attribute.HasAuthoredConnections() or attribute.GetNumTimeSamples():
            raise ValueError("connections and animation are unsupported")
        if attribute.GetTypeName() in (
            Sdf.ValueTypeNames.Asset,
            Sdf.ValueTypeNames.AssetArray,
        ):
            raise ValueError("visual source import is not supported yet")


def _attribute(prim: Usd.Prim, name: str) -> Any:
    value = prim.GetAttribute(name).Get()
    if value is None:
        raise ValueError(f"missing USD attribute {name} at {prim.GetPath()}")
    return value
