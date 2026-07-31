# blackflower-rendering-models

This crate owns Blackflower's validated runtime formats for cooked static
meshes and model hierarchies. It does not parse glTF and does not link
meshoptimizer. Authoring import, optimization, LOD generation, scene
selection, and attachment resolution belong to `tools/xtask`; the runtime sees
only deterministic `BFMESH` and `BFMODEL` payloads stored in the asset VFS.

Each mesh contains source-ordered primitives. A primitive retains its optional
glTF material index and authored vertex-channel mask, followed by a base mesh
and zero or more successively coarser LODs. Every LOD contains a fixed vertex
layout, triangle-list indices, object-space bounds, and the accumulated
meshoptimizer geometric error.

The first format revision stores:

- position as three `f32` values
- optional normal as three `f32` values
- optional tangent and handedness as four `f32` values
- optional first texture coordinate as two `f32` values
- indices as little-endian `u32` values

Absent optional channels contain zeroes and are distinguished by the primitive
attribute mask. `MeshAsset::from_bytes` validates the identifier, version,
counts, finite values, bounds, index ranges, decreasing LOD sizes, and
non-decreasing errors before exposing any mesh data.

```rust,no_run
use blackflower_assets::{AssetId, AssetStore};
use blackflower_rendering_models::MeshAsset;

# fn load(store: &AssetStore, id: &AssetId) -> Result<(), Box<dyn std::error::Error>> {
let mesh = MeshAsset::from_bytes(store.read_asset(id)?)?;
let base_lod = &mesh.primitives()[0].lods()[0];
println!(
    "{} vertices, {} indices",
    base_lod.vertices().len(),
    base_lod.indices().len()
);
# Ok(())
# }
```

`ModelAsset` preserves every node in the selected glTF scene in parent-before-
child depth-first order. Nodes retain their optional authored name and their
original local transform representation: decomposed TRS remains TRS and an
authored matrix remains a matrix. Attachments contain a resolved node index,
the complete logical `AssetId`, and an inferred Mesh or Volume kind.

The `.bfmodel` decoder rejects malformed forests, non-finite transforms,
non-normalized quaternions, out-of-range nodes, non-canonical attachment
ordering, repeated node/asset pairs, and more than one mesh on a node.
