use std::path::Path;

use anyhow::Context;
use serde::Deserialize;

/// Axis-aligned bounding box (world-space corners).
///
/// Solid geometry is reduced to these to feed the physics collider set.
#[derive(Clone, Copy, Debug)]
pub struct Aabb {
    pub min: [f32; 3],
    pub max: [f32; 3],
}

/// One map entity: a `classname` plus opaque string key/value `props`.
///
/// Quake style. The engine interprets only the classnames it knows (solids and
/// spawns); everything else is passed through untouched for gameplay/plugins.
#[derive(Clone, Debug, Deserialize)]
pub struct MapEntity {
    pub classname: String,
    #[serde(default)]
    pub props: Vec<(String, String)>,
}

impl MapEntity {
    /// Value of property `key`, if present.
    #[must_use]
    pub fn prop(&self, key: &str) -> Option<&str> {
        self.props
            .iter()
            .find(|(k, _)| k == key)
            .map(|(_, v)| v.as_str())
    }
}

/// Static arena loaded from a RON file: an `id` and a flat list of entities.
#[derive(Clone, Debug, Deserialize)]
pub struct Arena {
    pub id: String,
    pub entities: Vec<MapEntity>,
}

/// Classnames the engine treats as solid geometry (AABB via `mins`/`maxs`).
const SOLID_CLASSNAMES: &[&str] = &["func_wall"];
/// Classnames the engine treats as player spawn points (`origin`).
const SPAWN_CLASSNAMES: &[&str] = &["info_player_deathmatch", "info_player_start"];

impl Arena {
    pub fn load<P>(path: P) -> anyhow::Result<Self>
    where
        P: AsRef<Path>,
    {
        let src = std::fs::read_to_string(path.as_ref())
            .with_context(|| format!("reading arena file {}", path.as_ref().display()))?;
        ron::from_str(&src)
            .map_err(|e| anyhow::anyhow!("arena parse error in {}: {e}", path.as_ref().display()))
    }

    /// Solid AABBs derived from solid entities (`mins`/`maxs` props).
    #[must_use]
    pub fn solids(&self) -> Vec<Aabb> {
        self.entities
            .iter()
            .filter(|e| SOLID_CLASSNAMES.contains(&e.classname.as_str()))
            .filter_map(|e| {
                let min = parse_vec3(e.prop("mins")?)?;
                let max = parse_vec3(e.prop("maxs")?)?;
                Some(Aabb { min, max })
            })
            .collect()
    }

    /// Player spawn origins derived from spawn entities (`origin` prop).
    #[must_use]
    pub fn spawn_points(&self) -> Vec<[f32; 3]> {
        self.entities
            .iter()
            .filter(|e| SPAWN_CLASSNAMES.contains(&e.classname.as_str()))
            .filter_map(|e| parse_vec3(e.prop("origin")?))
            .collect()
    }
}

/// Parse a `"x y z"` string of whitespace-separated floats into `[f32; 3]`.
fn parse_vec3(text: &str) -> Option<[f32; 3]> {
    let mut parts = text.split_whitespace();
    let x = parts.next()?.parse().ok()?;
    let y = parts.next()?.parse().ok()?;
    let z = parts.next()?.parse().ok()?;
    parts.next().is_none().then_some([x, y, z])
}
