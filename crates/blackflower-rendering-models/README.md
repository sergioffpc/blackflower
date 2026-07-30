# blackflower-rendering-models

This crate owns Blackflower's validated runtime format for cooked static
meshes. It does not parse glTF and does not link meshoptimizer. Authoring
import, optimization, and LOD generation belong to `tools/xtask`; the runtime
sees only the deterministic `BFMESH` payload stored in the asset VFS.

Each model contains source-ordered primitives. A primitive retains its optional
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
attribute mask. `ModelAsset::from_bytes` validates the identifier, version,
counts, finite values, bounds, index ranges, decreasing LOD sizes, and
non-decreasing errors before exposing any mesh data.

```rust,no_run
use blackflower_assets::{AssetId, AssetStore};
use blackflower_rendering_models::ModelAsset;

# fn load(store: &AssetStore, id: &AssetId) -> Result<(), Box<dyn std::error::Error>> {
let model = ModelAsset::from_bytes(store.read_asset(id)?)?;
let base_lod = &model.primitives()[0].lods()[0];
println!(
    "{} vertices, {} indices",
    base_lod.vertices().len(),
    base_lod.indices().len()
);
# Ok(())
# }
```
