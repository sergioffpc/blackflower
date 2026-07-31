# Blackflower navigation cooker

`blackflower-cooker-navigation` is the host-only Stage 6 bridge from authored
glTF/GLB geometry to tiled Recast/Detour data. It imports only nodes carrying
typed Blackflower navigation metadata, applies their world transforms, bakes
with the physical agent and complete Recast settings from `asset.toml`, and
emits the versioned `.bfnav` runtime container.

The crate is intentionally not a runtime dependency. Recast and
`dtCreateNavMeshData` are linked only into this cooker; the
`blackflower-navigation` runtime continues to link Detour query code only.

Area keys are assigned Detour IDs by the manifest's canonical alphabetical
order. Traversable polygons receive flag bit 0 and their authored costs are
compiled into a native `QueryFilter`. Blocked polygons receive no flags. No Lua
script participates in cooking or path queries.
