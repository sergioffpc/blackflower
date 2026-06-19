use serde::{Deserialize, Serialize};

pub const PROTOCOL_VERSION: u32 = 1;

#[derive(Clone, Copy, Debug, Serialize, Deserialize)]
pub struct Command {
    pub tick: u64,
    pub buttons: u64,
    /// Absolute view angles (radians): yaw about +Y, pitch about +X. Sent
    /// absolute (not as deltas) so a dropped datagram never desyncs orientation.
    pub yaw: f32,
    pub pitch: f32,
    pub snapshot_ack_tick: u64,
    pub snapshot_ack_bits: u32,
}

#[derive(Clone, Debug, Serialize, Deserialize)]
pub struct WorldDelta {
    pub tick: u64,
    pub ack: u64,
    pub baseline: u64,
    pub removed: Box<[u64]>,
    pub entities: Box<[EntityDelta]>,
}

#[derive(Clone, Debug, Serialize, Deserialize)]
pub struct EntityDelta {
    pub id: u64,
    pub translation: Option<[f32; 3]>,
    pub rotation: Option<[f32; 4]>,
    pub properties: PropertyDelta,
}

#[derive(Clone, Debug, Serialize, Deserialize)]
pub struct PropertyDelta {
    /// Props that changed since the baseline; engine merges by id.
    pub changed_props: Properties,
    /// Prop ids removed since the baseline.
    pub removed_props: Vec<u16>,
}

/// One engine-opaque entity property: `(id, raw bytes)`. The engine stores and
/// forwards the bytes without interpreting them — encoding is owned by the
/// game plugin.
pub type Property = (u16, Vec<u8>);
pub type Properties = Vec<Property>;

#[derive(Clone, Debug, Serialize, Deserialize)]
pub struct WorldSnapshot {
    pub entities: Box<[EntitySnapshot]>,
}

#[derive(Clone, Debug, Serialize, Deserialize)]
pub struct EntitySnapshot {
    pub id: u64,
    pub translation: [f32; 3],
    pub rotation: [f32; 4],
    pub properties: Properties,
}

#[derive(Clone, Debug, Serialize, Deserialize)]
pub enum RejectReason {
    VersionMismatch { server_version: u32 },
    ServerFull,
}

#[derive(Clone, Debug, Serialize, Deserialize)]
pub enum Request {
    Hello { protocol_version: u32 },
    Ping { client_send_ns: u64 },
}

#[derive(Clone, Debug, Serialize, Deserialize)]
pub enum Event {
    Welcome {
        tick_hz: u64,
        assigned_entity_id: u64,
    },
    Rejected {
        reason: RejectReason,
    },
    Pong {
        client_send_ns: u64,
        server_tick: u64,
    },
}
