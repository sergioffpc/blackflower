//! Pure ray-vs-AABB intersection for server-side hitscan.
//!
//! Server-authoritative, non-predicted (the client never raycasts for hits).

use blackflower_math::Vec3;

/// Intersect a ray with an axis-aligned box using the slab method.
///
/// Returns the entry distance `t >= 0` along `dir` (need not be normalised)
/// if the ray starting at `origin` hits the box `[min, max]`, else `None`.
/// A ray originating inside the box hits at `t == 0`.
#[must_use]
pub fn ray_aabb(origin: Vec3, dir: Vec3, min: Vec3, max: Vec3) -> Option<f32> {
    let mut t_enter = f32::NEG_INFINITY;
    let mut t_exit = f32::INFINITY;

    for axis in 0..3 {
        let o = origin[axis];
        let d = dir[axis];
        let (lo, hi) = (min[axis], max[axis]);
        if d.abs() < f32::EPSILON {
            // Ray parallel to this slab: miss if the origin is outside it.
            if o < lo || o > hi {
                return None;
            }
        } else {
            let inv = 1.0 / d;
            let mut t0 = (lo - o) * inv;
            let mut t1 = (hi - o) * inv;
            if t0 > t1 {
                core::mem::swap(&mut t0, &mut t1);
            }
            t_enter = t_enter.max(t0);
            t_exit = t_exit.min(t1);
            if t_enter > t_exit {
                return None;
            }
        }
    }

    // The box is behind the ray origin.
    if t_exit < 0.0 {
        return None;
    }
    Some(t_enter.max(0.0))
}

/// Nearest target whose AABB the ray enters first.
///
/// Each target is an `(id, center)` pair sharing the same `half`-extents; the
/// `id` type is opaque to physics so callers (e.g. the ECS) keep their own
/// identity type. Returns the `id` with the smallest entry distance, or `None`
/// if the ray hits nothing. The caller excludes the shooter from `targets`.
pub fn nearest_hit<T>(
    origin: Vec3,
    dir: Vec3,
    half: Vec3,
    targets: impl IntoIterator<Item = (T, Vec3)>,
) -> Option<T> {
    targets
        .into_iter()
        .filter_map(|(id, center)| {
            ray_aabb(origin, dir, center - half, center + half).map(|dist| (dist, id))
        })
        .min_by(|(a, _), (b, _)| a.total_cmp(b))
        .map(|(_, id)| id)
}
