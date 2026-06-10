//! Wire message types exchanged between client and server.

use serde::{Deserialize, Serialize};

#[derive(Clone, Debug, Serialize, Deserialize)]
pub struct Command {
    pub tick: u64,
    pub buttons: u64,
}

/// A snapshot of the entire simulation state at a specific tick.
#[derive(Clone, Debug, Serialize, Deserialize)]
pub struct Snapshot {
    pub tick: u64,
    pub last_processed_client_tick: u64,
    pub entities: Box<[EntitySnapshot]>,
}

/// Replicated state of a single entity.
#[derive(Clone, Debug, Serialize, Deserialize)]
pub struct EntitySnapshot {
    pub id: u64,
    pub translation: [f32; 3],
    pub rotation: [f32; 4],
}

/// Messages sent from the client to the server.
#[derive(Clone, Debug, Serialize, Deserialize)]
pub enum Request {
    /// Tell the server we are ready to receive snapshots.
    Hello,
}

/// Messages sent from the server to the client over the control stream.
#[derive(Clone, Debug, Serialize, Deserialize)]
pub enum Event {
    /// Confirms connection and assigns the client's avatar.
    Welcome { assigned_entity: u64 },
}
