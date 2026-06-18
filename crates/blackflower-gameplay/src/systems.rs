//! Predicted game rules.
//!
//! Pure functions run identically by server (`blackflower-authority`) and
//! client (`blackflower-replica` prediction). Any rule the client must predict
//! belongs here, never in the plugin. See ADR 0017 in `docs/ARCHITECTURE.md`.

use blackflower_input::components::InputButtons;
use blackflower_math::{Quat, Vec2, Vec3, components::Transform};

const PLAYER_MOVE_SPEED: f32 = 5.0;

/// Set the player's orientation from absolute view angles (radians).
///
/// `yaw` rotates about +Y, then `pitch` about +X. Pure — the caller (server or
/// client prediction) clamps pitch before producing the command. Apply this
/// *before* [`apply_player_movement`] each tick so movement uses the new facing.
pub fn apply_player_look(transform: &mut Transform, yaw: f32, pitch: f32) {
    transform.rotation = Quat::from_rotation_y(yaw) * Quat::from_rotation_x(pitch);
}

/// Move the player relative to its facing: WASD maps to forward/back/strafe in
/// the horizontal plane derived from the transform's yaw (pitch never lifts
/// movement off the ground).
pub fn apply_player_movement(transform: &mut Transform, buttons: InputButtons, dt: f32) {
    let dir = buttons.normalize_or_zero();
    if dir == Vec2::ZERO {
        return;
    }
    // Facing flattened onto the ground plane; `right` is orthogonal to it.
    let facing = transform.rotation * Vec3::NEG_Z;
    let forward = Vec3::new(facing.x, 0.0, facing.z).normalize_or_zero();
    if forward == Vec3::ZERO {
        return; // looking straight up/down — no horizontal facing to move along
    }
    let right = forward.cross(Vec3::Y);
    // dir.y is +1 for BACKWARD, -1 for FORWARD; dir.x is +1 for RIGHT.
    let motion = forward * -dir.y + right * dir.x;
    transform.translation += motion * PLAYER_MOVE_SPEED * dt;
}
