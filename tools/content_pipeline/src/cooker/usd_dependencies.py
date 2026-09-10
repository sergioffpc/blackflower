"""Capture the bounded scene/entity layer graph before USD composition."""

import dataclasses
import hashlib
import os
import pathlib
import struct

from pxr import Gf
from pxr import Sdf
from pxr import Usd
from pxr import UsdGeom


@dataclasses.dataclass
class Snapshot:
    """Private root layer and canonical dependency transcript."""

    root: pathlib.Path
    transcript: bytes


def capture(source: pathlib.Path, directory: pathlib.Path) -> Snapshot:
    """Snapshots local entity references and returns their source transcript.

    Args:
        source: Scene root filename; inputs must remain unchanged during cook.
        directory: Private temporary directory owned by the caller.

    Returns:
        Private input graph root and deterministic provenance data.

    Raises:
        ValueError: Layer composition is outside the supported subset.
        OSError: Reading or writing a dependency fails.
    """
    source = pathlib.Path(os.path.abspath(source))
    files = {source: source.read_bytes()}
    root = _parse(files[source], directory / "root.usd")
    for reference in _references(root, definition=False):
        path = pathlib.Path(
            os.path.abspath(source.parent / reference.assetPath)
        )
        if path == source:
            raise ValueError("entity reference cannot target the scene")
        if path not in files:
            files[path] = path.read_bytes()
            layer = _parse(files[path], directory / f"entity-{len(files)}.usd")
            _references(layer, definition=True)
            _validate_definition(layer)
    common = pathlib.Path(os.path.commonpath([p.parent for p in files]))
    for path, raw in files.items():
        target = directory / "graph" / path.relative_to(common)
        target.parent.mkdir(parents=True, exist_ok=True)
        target.write_bytes(raw)
    return Snapshot(
        root=directory / "graph" / source.relative_to(common),
        transcript=_transcript(source, files),
    )


def _parse(raw: bytes, target: pathlib.Path) -> Sdf.Layer:
    target.write_bytes(raw)
    layer = Sdf.Layer.FindOrOpen(str(target))
    if not layer or layer.subLayerPaths:
        raise ValueError("USD sublayers are not supported")
    return layer


def _references(layer: Sdf.Layer, *, definition: bool) -> list[Sdf.Reference]:
    references: list[Sdf.Reference] = []

    def visit(path: Sdf.Path | str) -> None:
        path = Sdf.Path(path)
        spec = layer.GetObjectAtPath(path)
        if not isinstance(spec, Sdf.PrimSpec):
            return
        if (
            spec.payloadList.GetAppliedItems()
            or spec.inheritPathList.GetAppliedItems()
            or spec.specializesList.GetAppliedItems()
            or spec.variantSets
            or spec.instanceable
        ):
            raise ValueError(f"unsupported USD composition at {path}")
        items = spec.referenceList.GetAppliedItems()
        if items:
            if definition or str(path.GetParentPath()) != "/Scene/Entities":
                raise ValueError(
                    "references are only allowed on scene entities"
                )
            if len(items) != 1:
                raise ValueError(
                    "each entity requires one definition reference"
                )
            _validate_reference(items[0])
            references.append(items[0])
        if definition and "blackflower:id" in spec.attributes:
            raise ValueError("entity definitions cannot supply placement IDs")

    layer.Traverse(Sdf.Path.absoluteRootPath, visit)
    expected = "Entity" if definition else "Scene"
    if layer.defaultPrim != expected:
        raise ValueError(f"USD default prim must be {expected}")
    return references


def _validate_definition(layer: Sdf.Layer) -> None:
    stage = Usd.Stage.Open(layer, load=Usd.Stage.LoadNone)
    root = stage.GetDefaultPrim()
    if (
        len(layer.rootPrims) != 1
        or root.GetTypeName() != "Xform"
        or UsdGeom.Xformable(root).GetLocalTransformation() != Gf.Matrix4d(1)
        or not stage.HasAuthoredMetadata("metersPerUnit")
        or UsdGeom.GetStageMetersPerUnit(stage) != 1
        or UsdGeom.GetStageUpAxis(stage) != UsdGeom.Tokens.y
    ):
        raise ValueError(
            "entity definitions require identity Entity Xform, metres and Y up"
        )


def _validate_reference(reference: Sdf.Reference) -> None:
    path = pathlib.PurePosixPath(reference.assetPath)
    if (
        not reference.assetPath
        or path.is_absolute()
        or "\\" in reference.assetPath
        or ":" in reference.assetPath
        or reference.primPath not in (Sdf.Path.emptyPath, Sdf.Path("/Entity"))
        or reference.layerOffset != Sdf.LayerOffset()
    ):
        raise ValueError("entity references require relative local layer paths")


def _transcript(
    source: pathlib.Path, files: dict[pathlib.Path, bytes]
) -> bytes:
    records = {
        ".": files[source],
        **{
            os.path.relpath(path, source.parent).replace(os.sep, "/"): raw
            for path, raw in files.items()
            if path != source
        },
    }
    result = bytearray(b"Blackflower.USDSource.v1\0")
    for name, raw in sorted(records.items()):
        encoded = name.encode("utf-8")
        result.extend(struct.pack("<I", len(encoded)))
        result.extend(encoded)
        result.extend(struct.pack("<Q", len(raw)))
        result.extend(hashlib.sha256(raw).digest())
    return bytes(result)
