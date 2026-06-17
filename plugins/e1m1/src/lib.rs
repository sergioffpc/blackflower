wit_bindgen::generate!({
    path: "../../wit/game-plugin.wit",
    world: "game-plugin",
});

/// Property schema (shared convention between plugin and game clients).
/// The engine never interprets these — only the plugin and client UI do.
const PROP_HP: u16 = 1;
const HP_INITIAL: i32 = 100;
const HP_DAMAGE: i32 = 25;

struct Plugin;

impl Guest for Plugin {
    fn on_spawn() -> Vec<(u16, Vec<u8>)> {
        vec![(PROP_HP, encode(HP_INITIAL))]
    }

    fn on_hit(target_props: Vec<(u16, Vec<u8>)>) -> Vec<(u16, Vec<u8>)> {
        target_props
            .into_iter()
            .map(|(id, val)| {
                if id == PROP_HP {
                    let hp = decode(&val);
                    (id, encode((hp - HP_DAMAGE).max(0)))
                } else {
                    (id, val)
                }
            })
            .collect()
    }
}

export!(Plugin);

fn encode(v: i32) -> Vec<u8> {
    v.to_le_bytes().to_vec()
}

fn decode(b: &[u8]) -> i32 {
    b.try_into().map_or(0, i32::from_le_bytes)
}
