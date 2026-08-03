# blackflower-cooker-vfx

Host-only boundary for validating authored visual effects and cooking them into
the backend-neutral runtime representation owned by `blackflower-vfx`.

The future cooker may resolve referenced textures, meshes, curves, and effect
graphs, canonicalize their ordering, enforce resource budgets, and emit the
complete dependency set required by the asset VFS. It will not simulate or
render effects and will not be linked into the game runtime.

The crate is deliberately a scaffold. No authoring format, cooked extension,
schema version, editor integration, or cooking API has been selected yet.
