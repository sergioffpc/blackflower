use serde::{Deserialize, Serialize};

#[derive(Clone, Copy, Debug, Serialize, Deserialize)]
pub struct Command {
    pub tick: u64,
    pub buttons: u64,
}

#[derive(Clone, Debug, Serialize, Deserialize)]
pub struct Snapshot {
    pub tick: u64,
    pub ack: u64,
    pub world: WorldSnapshot,
}

#[derive(Clone, Debug, Serialize, Deserialize)]
pub struct WorldSnapshot {
    pub entities: Box<[EntitySnapshot]>,
}

#[derive(Clone, Debug, Serialize, Deserialize)]
pub struct EntitySnapshot {
    pub id: u64,
    pub translation: [f32; 3],
    pub rotation: [f32; 4],
}

#[derive(Clone, Debug, Serialize, Deserialize)]
pub enum Request {
    Hello,
    Ping { client_send_ns: u64 },
}

#[derive(Clone, Debug, Serialize, Deserialize)]
pub enum Event {
    Welcome {
        tick_hz: u64,
        assigned_entity_id: u64,
    },
    Pong {
        client_send_ns: u64,
        server_tick: u64,
    },
}
