# Coordinate System

This document is the normative spatial convention for Blackflower engine
space. World, model, view-independent presentation, physics, navigation, and
listener-relative data use this convention unless an API explicitly names a
different local space.

## Handedness and engine-space basis

Blackflower uses a right-handed Cartesian coordinate system:

- `+X` points right.
- `+Y` points up.
- `-Z` points forward.
- `+Z` points backward.
- `X × Y = Z`.

The canonical basis vectors are therefore:

```text
right   = ( 1,  0,  0)
up      = ( 0,  1,  0)
forward = ( 0,  0, -1)
back    = ( 0,  0,  1)
```

Positive rotations follow the right-hand rule around the positive axis.
Linear engine-space distances are expressed in metres and angles in radians.
A format may use another explicit storage unit, such as millimetres, but that
does not change its axes or handedness.

Handedness does not define matrix memory layout, raster front-face winding,
clip-space depth, texture coordinates, or image origin. Those are separate
contracts. Cooked `.bfmodel` transforms are specifically column-major local
matrices.

## Coordinate boundaries

Foreign coordinate systems must be converted exactly once at their cooker or
runtime adapter boundary. Internal data must not carry an implicit foreign
basis or require callers to remember an undocumented axis swap.

glTF 2.0 uses a right-handed, Y-up basis with `+Z` forward and `-X` right.
Model and mesh cooking changes that basis to Blackflower with a 180-degree
rotation around Y:

```text
point_engine = (-point_gltf.x, point_gltf.y, -point_gltf.z)
C            = diagonal(-1, 1, -1, 1)
matrix_engine = C * matrix_gltf * inverse(C)
```

Because this basis change is a rotation with determinant `+1`, it preserves
handedness, triangle winding, and tangent-basis handedness. The model cooker
normalizes and sign-canonicalizes authored TRS quaternions before composing
the canonical matrix; authored matrices are basis-changed without
decomposition.

Steam Audio's spatial convention already matches Blackflower engine space:
`+X` right, `+Y` up, and `-Z` ahead. Its adapter therefore normalizes direction
length but does not change the coordinate basis.

The [glTF 2.0 coordinate-system specification][gltf-coordinates] is the
authoritative source for the glTF side of this boundary.

[gltf-coordinates]: https://registry.khronos.org/glTF/specs/2.0/glTF-2.0.html#coordinate-system-and-units

## Local-space exceptions

An API may define semantic axes in a named local space, such as a joint-local
aim axis. Such an exception does not change engine-space handedness. The API
must document the local basis and convert explicitly when values cross into
engine, model, or world space.
