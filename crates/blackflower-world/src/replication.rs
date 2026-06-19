use std::collections::VecDeque;

use blackflower_math::components::Transform;
use blackflower_time::Tick;
use hashbrown::{HashMap, HashSet};

use crate::EntityId;

const INTERP_HISTORY: usize = 8;

#[derive(Debug, Default)]
pub struct Replication {
    buffer: HashMap<EntityId, VecDeque<InterpolationSample>>,
}

impl Replication {
    pub fn latest_tick(&self) -> Tick {
        self.buffer
            .values()
            .filter_map(|buf| buf.back().map(|s| s.tick))
            .max()
            .unwrap_or(Tick::ZERO)
    }

    pub fn interpolation_track(&self, k: EntityId) -> InterpolationTrack {
        let samples = self
            .buffer
            .get(&k)
            .map(|buf| buf.iter().copied().collect::<Box<_>>())
            .unwrap_or_default();
        InterpolationTrack { samples }
    }

    pub fn push_sample(&mut self, k: EntityId, v: InterpolationSample) {
        let buf = self.buffer.entry(k).or_default();
        debug_assert!(
            buf.back().is_none_or(|s| s.tick < v.tick),
            "push_sample called with non-monotonic tick; apply must reject stale snapshots first"
        );
        if buf.len() == INTERP_HISTORY {
            buf.pop_front();
        }
        buf.push_back(v);
    }

    pub fn remove(&mut self, k: &EntityId) {
        self.buffer.remove(k);
    }

    pub fn retain_present_entities(&mut self, present: &HashSet<EntityId>) {
        self.buffer.retain(|id, _| present.contains(id));
    }
}

#[derive(Clone, Copy, Debug)]
pub struct InterpolationSample {
    pub tick: Tick,
    pub transform: Transform,
}

#[derive(Clone, Debug, Default)]
pub struct InterpolationTrack {
    samples: Box<[InterpolationSample]>,
}

impl InterpolationTrack {
    #[must_use]
    pub fn interpolate(&self, t: f64) -> Option<Transform> {
        match self.samples.as_ref() {
            [] => None,
            [only] => Some(only.transform),
            _ => self.interpolate_bracketed(t),
        }
    }

    fn interpolate_bracketed(&self, t: f64) -> Option<Transform> {
        let newest = self.samples[self.samples.len() - 1];
        let oldest = self.samples[0];
        if t >= newest.tick.as_f64() {
            return Some(newest.transform);
        }
        if t <= oldest.tick.as_f64() {
            return Some(oldest.transform);
        }
        self.samples.windows(2).find_map(|w| {
            let a = w[0];
            let b = w[1];
            let (lo, hi) = (a.tick.as_f64(), b.tick.as_f64());
            if t >= lo && t < hi {
                let span = hi - lo;
                let t = if span > 0.0 {
                    ((t - lo) / span) as f32
                } else {
                    0.0
                };
                Some(a.transform.lerp(b.transform, t))
            } else {
                None
            }
        })
    }
}
