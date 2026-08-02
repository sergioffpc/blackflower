/// Errors produced while creating or updating destructible state.
#[derive(Debug, Clone, PartialEq, Eq, thiserror::Error)]
pub enum Error {
    /// An asset must contain at least one valid chunk.
    #[error("invalid destruction chunk descriptor")]
    InvalidChunk,
    /// A bond descriptor has invalid geometry or chunk endpoints.
    #[error("invalid destruction bond descriptor")]
    InvalidBond,
    /// Initial or applied health must be finite and strictly positive.
    #[error("destruction health must be finite and strictly positive")]
    InvalidHealth,
    /// A direct fracture command refers to a chunk or support-graph node outside the asset.
    #[error("invalid destruction fracture target")]
    InvalidFractureTarget,
    /// A stress force refers to a support-graph node outside the asset.
    #[error("destruction support-graph node does not exist")]
    GraphNodeNotFound,
    /// Stress density or limits are invalid.
    #[error("invalid destruction stress settings")]
    InvalidStressSettings,
    /// Blast rejected the authored chunk hierarchy or bond graph.
    #[error("NVIDIA Blast rejected the destruction asset")]
    AssetCreation,
    /// Blast could not instance a mutable family from the asset.
    #[error("NVIDIA Blast could not create a destruction family")]
    FamilyCreation,
    /// The requested actor is no longer active in this family.
    #[error("destruction actor is not active")]
    ActorNotFound,
    /// Native memory could not be allocated.
    #[error("native destruction allocation failed")]
    AllocationFailed,
    /// The pinned stress solver does not support this target architecture.
    #[error("NvBlastExtStress is unavailable on this target")]
    StressUnavailable,
    /// Stress processing was requested before a solver was enabled or initialization failed.
    #[error("destruction stress solver is not ready")]
    StressNotReady,
    /// The private native wrapper violated an internal capacity or ABI contract.
    #[error("native destruction wrapper contract violation")]
    NativeContract,
}
