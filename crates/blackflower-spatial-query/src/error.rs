/// Spatial scene construction or query failure.
#[derive(Debug, thiserror::Error)]
pub enum Error {
    /// A triangle contains non-finite coordinates or is degenerate.
    #[error("invalid spatial-query triangle")]
    InvalidTriangle,
    /// A scene received no triangles for one geometry.
    #[error("spatial-query geometry is empty")]
    EmptyGeometry,
    /// A public count exceeds the native 32-bit API.
    #[error("spatial-query resource limit reached for {0}")]
    ResourceLimit(&'static str),
    /// A bounded query buffer could not reserve memory.
    #[error("spatial-query output allocation failed")]
    OutOfMemory,
    /// Native object allocation failed.
    #[error("Embree allocation failed")]
    NativeOutOfMemory,
    /// Embree rejected a scene operation or query.
    #[error("Embree operation failed")]
    NativeFailure,
    /// A committed immutable scene was modified or committed again.
    #[error("spatial-query scene is already committed")]
    SceneCommitted,
    /// The native wrapper violated its pointer or result contract.
    #[error("spatial-query native contract violation")]
    ContractViolation,
}
