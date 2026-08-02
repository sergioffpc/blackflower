//! Dedicated authoritative server composition.
//!
//! The executable owns deployment configuration. This library composes the
//! low-level QUIC endpoint with admission, bounded input ingress, replication,
//! and scheduling without defining gameplay command schemas.

mod network;

pub use network::{
    AdmittedSession, AuthenticatedVoiceCapture, ClassifiedCommand, DedicatedServerNetwork,
    InputIngress, NetworkPeer, PeerError, ResumeOutcome,
};
