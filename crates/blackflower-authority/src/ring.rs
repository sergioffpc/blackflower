use blackflower_protocol::WorldSnapshot;
use blackflower_time::Tick;

const RING_SIZE: usize = 32;

/// Fixed-size ring of the last `RING_SIZE` world snapshots, keyed by tick.
pub struct SnapshotRing {
    entries: [Option<(Tick, WorldSnapshot)>; RING_SIZE],
}

impl Default for SnapshotRing {
    fn default() -> Self {
        Self {
            entries: std::array::from_fn(|_| None),
        }
    }
}

impl SnapshotRing {
    pub fn insert(&mut self, tick: Tick, snapshot: WorldSnapshot) {
        self.entries[(tick.as_u64() % RING_SIZE as u64) as usize] = Some((tick, snapshot));
    }

    pub fn get(&self, tick: Tick) -> Option<&WorldSnapshot> {
        if tick == Tick::ZERO {
            return None;
        }
        let (stored_tick, snapshot) =
            self.entries[(tick.as_u64() % RING_SIZE as u64) as usize].as_ref()?;
        (*stored_tick == tick).then_some(snapshot)
    }
}
