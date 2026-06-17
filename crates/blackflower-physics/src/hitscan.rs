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

#[cfg(test)]
mod tests {
    use super::*;

    fn v(x: f32, y: f32, z: f32) -> Vec3 {
        Vec3::new(x, y, z)
    }

    #[test]
    fn hits_box_in_front() {
        let t = ray_aabb(
            v(0.0, 0.0, 0.0),
            v(1.0, 0.0, 0.0),
            v(4.0, -1.0, -1.0),
            v(6.0, 1.0, 1.0),
        );
        assert_eq!(t, Some(4.0));
    }

    #[test]
    fn misses_box_to_the_side() {
        let t = ray_aabb(
            v(0.0, 0.0, 0.0),
            v(1.0, 0.0, 0.0),
            v(4.0, 5.0, 5.0),
            v(6.0, 7.0, 7.0),
        );
        assert_eq!(t, None);
    }

    #[test]
    fn misses_box_behind() {
        let t = ray_aabb(
            v(0.0, 0.0, 0.0),
            v(1.0, 0.0, 0.0),
            v(-6.0, -1.0, -1.0),
            v(-4.0, 1.0, 1.0),
        );
        assert_eq!(t, None);
    }

    #[test]
    fn origin_inside_hits_at_zero() {
        let t = ray_aabb(
            v(0.0, 0.0, 0.0),
            v(1.0, 0.0, 0.0),
            v(-1.0, -1.0, -1.0),
            v(1.0, 1.0, 1.0),
        );
        assert_eq!(t, Some(0.0));
    }
}
