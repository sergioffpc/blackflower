//! Dedicated authoritative server composition.
//!
//! The executable owns deployment configuration. This library composes the
//! low-level QUIC endpoint with admission, bounded input ingress, replication,
//! scheduling, and optional interactive foreground diagnostics without
//! defining gameplay command schemas.

pub mod foreground;
mod network;

pub use network::{
    AdmittedSession, AuthenticatedVoiceCapture, ClassifiedCommand, DedicatedServerNetwork,
    InputIngress, NetworkPeer, PeerError, ResumeOutcome,
};
