use std::sync::atomic::{AtomicUsize, Ordering};

wit_bindgen::generate!({
    path: "../../wit/game-plugin.wit",
    world: "game-plugin",
});

/// Property schema (shared convention between plugin and game clients).
/// The engine never interprets these — only the plugin and client UI do.
const PROP_HP: u16 = 1;
const HP_INITIAL: i32 = 100;
const HP_DAMAGE: i32 = 25;

/// Cursor for round-robin spawn selection across this match.
static NEXT_SPAWN: AtomicUsize = AtomicUsize::new(0);

struct Plugin;

impl Guest for Plugin {
    fn select_spawn(candidates: Vec<SpawnPoint>) -> u32 {
        if candidates.is_empty() {
            return 0;
        }
        let idx = NEXT_SPAWN.fetch_add(1, Ordering::Relaxed) % candidates.len();
        idx as u32
    }

    fn on_spawn() -> Vec<(u16, Vec<u8>)> {
        vec![(PROP_HP, encode(HP_INITIAL))]
    }

    fn on_hit(target_props: Vec<(u16, Vec<u8>)>) -> HitResult {
        let mut respawn = false;
        let props = target_props
            .into_iter()
            .map(|(id, val)| {
                if id == PROP_HP {
                    let hp = (decode(&val) - HP_DAMAGE).max(0);
                    respawn = respawn || hp == 0;
                    (id, encode(hp))
                } else {
                    (id, val)
                }
            })
            .collect();
        HitResult { props, respawn }
    }
}

export!(Plugin);

fn encode(v: i32) -> Vec<u8> {
    v.to_le_bytes().to_vec()
}

fn decode(b: &[u8]) -> i32 {
    b.try_into().map_or(0, i32::from_le_bytes)
}
