use blackflower_input::components::InputButtons;
use blackflower_math::{Vec2, Vec3, components::Transform};

const PLAYER_MOVE_SPEED: f32 = 5.0;

pub fn apply_player_movement(transform: &mut Transform, buttons: InputButtons, dt: f32) {
    let dir = buttons.normalize_or_zero();
    if dir == Vec2::ZERO {
        return;
    }
    transform.translation += Vec3::new(dir.x, 0.0, dir.y) * PLAYER_MOVE_SPEED * dt;
}
