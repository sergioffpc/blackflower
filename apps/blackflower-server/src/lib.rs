//! Dedicated authoritative server composition.
//!
//! The executable owns deployment configuration and the fixed-rate
//! authoritative [`SimulationHost`]. This library composes the low-level QUIC
//! endpoint with admission, bounded input ingress, replication, scheduling,
//! and optional interactive foreground diagnostics without defining gameplay
//! command schemas.

mod admission;
pub mod foreground;
mod network;
mod network_runtime;
mod projection;
mod simulation;

pub use admission::LoopbackSessionAuthority;
pub use network::{
    AdmittedSession, AuthenticatedVoiceCapture, ClassifiedCommand, DedicatedServerNetwork,
    InputIngress, NetworkPeer, PeerError, ResumeOutcome,
};
pub use network_runtime::{ServerNetworkRuntime, ServerNetworkRuntimeError};
pub use projection::{SimulationProjectionError, project_movement_frame};
pub use simulation::{
    SimulationExit, SimulationHost, SimulationHostError, SimulationIngressError, SimulationStatus,
};
