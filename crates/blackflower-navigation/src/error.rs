/// Errors produced while loading or querying a Detour navigation mesh.
#[derive(Debug, Clone, PartialEq, Eq, thiserror::Error)]
pub enum Error {
    /// The tile bytes are empty, truncated, or incompatible with Detour 1.6.0.
    #[error("invalid Detour navigation mesh tile data")]
    InvalidNavMeshData,
    /// Tiled navigation mesh parameters are invalid or exceed Detour limits.
    #[error("invalid Detour tiled navigation mesh parameters")]
    InvalidNavMeshParameters,
    /// Detour could not allocate native navigation state.
    #[error("Detour navigation allocation failed")]
    AllocationFailed,
    /// Detour could not initialize the navigation mesh.
    #[error("Detour navigation mesh initialization failed")]
    NavMeshInitialization,
    /// The configured tiled navigation mesh has no remaining tile capacity.
    #[error("Detour navigation mesh tile capacity exhausted")]
    TileCapacityExhausted,
    /// A tile already occupies the baked grid coordinate and layer.
    #[error("a Detour tile already occupies that grid coordinate and layer")]
    TileAlreadyOccupied,
    /// A world-space vector must contain only finite components.
    #[error("navigation vector components must be finite")]
    InvalidVector,
    /// Nearest-polygon query half extents must be finite and strictly positive.
    #[error("navigation query half extents must be finite and strictly positive")]
    InvalidQueryExtents,
    /// Area identifiers must be below [`crate::MAX_AREAS`].
    #[error("navigation area {0} is outside Detour's supported range")]
    InvalidArea(u8),
    /// Traversal costs must be finite and strictly positive.
    #[error("navigation area traversal cost must be finite and strictly positive")]
    InvalidAreaCost,
    /// A query node capacity exceeds Detour's limit.
    #[error("navigation query node capacity {0} exceeds Detour's limit")]
    QueryNodeCapacityTooLarge(u32),
    /// A result capacity cannot be represented by Detour.
    #[error("navigation result capacity {0} exceeds Detour's limit")]
    ResultCapacityTooLarge(u32),
    /// Detour could not initialize a query object.
    #[error("Detour navigation query initialization failed")]
    QueryInitialization,
    /// No traversable polygon was found around the requested path start.
    #[error("no navigation polygon found around the path start")]
    StartPolygonNotFound,
    /// No traversable polygon was found around the requested path end.
    #[error("no navigation polygon found around the path end")]
    EndPolygonNotFound,
    /// A polygon handle belongs to a different navigation mesh.
    #[error("polygon handle belongs to a different navigation mesh")]
    WrongNavMesh,
    /// A polygon handle is no longer valid for its navigation mesh.
    #[error("navigation polygon handle is invalid")]
    InvalidPolygon,
    /// Detour exhausted the query's search-node pool.
    #[error("Detour navigation query exhausted its node pool")]
    QueryOutOfNodes,
    /// The configured polygon corridor capacity was too small.
    #[error("navigation path polygon capacity exhausted")]
    PathCapacityExceeded,
    /// The configured straight-path point capacity was too small.
    #[error("navigation straight-path point capacity exhausted")]
    StraightPathCapacityExceeded,
    /// The configured raycast polygon capacity was too small.
    #[error("navigation raycast polygon capacity exhausted")]
    RaycastCapacityExceeded,
    /// Detour failed a query without a more specific public reason.
    #[error("Detour navigation query failed")]
    QueryFailed,
    /// The private native wrapper rejected an internally valid call.
    #[error("native Detour wrapper contract violation")]
    NativeContract,
}
