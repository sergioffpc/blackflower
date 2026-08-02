use glam::Vec3A;

/// Stable index of an active actor inside one Blast family.
#[derive(Clone, Copy, Debug, Eq, Ord, PartialEq, PartialOrd, Hash)]
pub struct ActorId(u32);

impl ActorId {
    /// Returns the family-local actor index.
    #[must_use]
    pub const fn get(self) -> u32 {
        self.0
    }

    pub(crate) const fn from_native(value: u32) -> Self {
        Self(value)
    }
}

/// Stable node index in the asset support graph.
#[derive(Clone, Copy, Debug, Eq, Ord, PartialEq, PartialOrd, Hash)]
pub struct GraphNodeId(u32);

impl GraphNodeId {
    /// Creates a support-graph node identifier.
    #[must_use]
    pub const fn new(value: u32) -> Self {
        Self(value)
    }

    /// Returns the support-graph node index.
    #[must_use]
    pub const fn get(self) -> u32 {
        self.0
    }
}

/// Authored chunk used to create one immutable destruction asset.
#[derive(Clone, Copy, Debug, PartialEq)]
pub struct ChunkDesc {
    /// Chunk centroid in asset-local metres.
    pub centroid: Vec3A,
    /// Chunk volume in cubic metres.
    pub volume: f32,
    /// Parent chunk, or `None` for a root.
    pub parent: Option<u32>,
    /// Whether the chunk participates directly in the support graph.
    pub support: bool,
    /// Domain-owned stable metadata copied into fracture events.
    pub user_data: u32,
}

/// Authored connection between two support chunks or a chunk and the world.
#[derive(Clone, Copy, Debug, PartialEq)]
pub struct BondDesc {
    /// Average interface normal in asset-local space.
    pub normal: Vec3A,
    /// Interface area in square metres.
    pub area: f32,
    /// Interface centroid in asset-local metres.
    pub centroid: Vec3A,
    /// Connected chunk indices. `None` denotes the external world.
    pub chunks: [Option<u32>; 2],
    /// Domain-owned stable metadata copied into fracture events.
    pub user_data: u32,
}

/// Direct damage to apply to a Blast actor.
#[derive(Clone, Copy, Debug, PartialEq)]
pub enum FractureCommand {
    /// Damages the bond between two support-graph nodes.
    Bond {
        /// First support-graph node.
        first: GraphNodeId,
        /// Second support-graph node.
        second: GraphNodeId,
        /// Positive health removed from the bond.
        damage: f32,
    },
    /// Damages one asset chunk.
    Chunk {
        /// Asset chunk index.
        chunk_index: u32,
        /// Positive health removed from the chunk.
        damage: f32,
    },
}

/// Result reported after applying fracture commands.
#[derive(Clone, Copy, Debug, PartialEq)]
pub enum FractureEvent {
    /// Updated health for a bond. Negative health denotes excess damage.
    Bond {
        /// First support-graph node.
        first: GraphNodeId,
        /// Second support-graph node.
        second: GraphNodeId,
        /// Authored domain metadata.
        user_data: u32,
        /// Remaining health after damage.
        remaining_health: f32,
    },
    /// Updated health for a chunk. Negative health denotes excess damage.
    Chunk {
        /// Asset chunk index.
        chunk_index: u32,
        /// Authored domain metadata.
        user_data: u32,
        /// Remaining health after damage.
        remaining_health: f32,
    },
}

/// Interpretation of a stress-solver vector.
#[derive(Clone, Copy, Debug, Eq, PartialEq)]
pub enum ForceMode {
    /// Newtons, scaled by node mass.
    Force,
    /// Metres per second squared, independent of node mass.
    Acceleration,
}

/// Runtime limits for `NvBlastExtStress`.
#[derive(Clone, Copy, Debug, PartialEq)]
pub struct StressSettings {
    /// Maximum iterative solver work per update.
    pub max_solver_iterations_per_frame: u32,
    /// Number of support-graph reduction passes.
    pub graph_reduction_level: u32,
    /// Compression below which no damage is produced, in pascals.
    pub compression_elastic_limit: f32,
    /// Compression that immediately breaks a bond, in pascals.
    pub compression_fatal_limit: f32,
    /// Tension elastic limit, or a negative value to use compression.
    pub tension_elastic_limit: f32,
    /// Tension fatal limit, or a negative value to use compression.
    pub tension_fatal_limit: f32,
    /// Shear elastic limit, or a negative value to use compression.
    pub shear_elastic_limit: f32,
    /// Shear fatal limit, or a negative value to use compression.
    pub shear_fatal_limit: f32,
}

impl Default for StressSettings {
    fn default() -> Self {
        Self {
            max_solver_iterations_per_frame: 25,
            graph_reduction_level: 0,
            compression_elastic_limit: 1.0,
            compression_fatal_limit: 2.0,
            tension_elastic_limit: -1.0,
            tension_fatal_limit: -1.0,
            shear_elastic_limit: -1.0,
            shear_fatal_limit: -1.0,
        }
    }
}

/// Telemetry from the most recent stress update.
#[derive(Clone, Copy, Debug, PartialEq)]
pub struct StressStats {
    /// Number of updates since solver creation or reset.
    pub frame_count: u32,
    /// Number of bonds after graph reduction.
    pub bond_count: u32,
    /// Bonds recommended for fracture.
    pub overstressed_bond_count: u32,
    /// Current linear residual.
    pub linear_error: f32,
    /// Current angular residual.
    pub angular_error: f32,
    /// Whether the iterative solve converged.
    pub converged: bool,
}
