use glam::Vec2;
use winit::dpi::PhysicalPosition;
use winit::event::{ElementState, MouseButton, MouseScrollDelta};
use winit::keyboard::{KeyCode, ModifiersState, PhysicalKey};

use super::{InputContext, InputState};

#[test]
fn focus_loss_neutralizes_every_device() {
    let mut input = InputState::default();
    input.set_focused(true);
    input.set_context(InputContext::GameplayCaptured);
    input.keyboard_input(
        PhysicalKey::Code(KeyCode::KeyW),
        ElementState::Pressed,
        false,
    );
    input.mouse_input(MouseButton::Left, ElementState::Pressed);
    input.raw_mouse_motion(Vec2::new(3.0, -2.0));
    input.modifiers_changed(ModifiersState::SHIFT);

    input.set_focused(false);
    let snapshot = input.take_snapshot();

    assert!(!snapshot.focused());
    assert_eq!(snapshot.context(), InputContext::UserInterface);
    assert!(!snapshot.key_held(KeyCode::KeyW));
    assert!(!snapshot.mouse_button_held(MouseButton::Left));
    assert_eq!(snapshot.modifiers(), ModifiersState::empty());
    assert_vector_close(snapshot.relative_mouse_motion(), Vec2::ZERO);
}

#[test]
fn synthetic_focus_press_is_ignored() {
    let mut input = InputState::default();
    input.set_focused(true);
    input.keyboard_input(
        PhysicalKey::Code(KeyCode::KeyW),
        ElementState::Pressed,
        true,
    );

    let snapshot = input.take_snapshot();
    assert!(!snapshot.key_held(KeyCode::KeyW));
    assert!(!snapshot.key_pressed(KeyCode::KeyW));
}

#[test]
fn snapshots_consume_edges_but_preserve_holds() {
    let mut input = InputState::default();
    input.set_focused(true);
    input.keyboard_input(
        PhysicalKey::Code(KeyCode::Space),
        ElementState::Pressed,
        false,
    );

    let first = input.take_snapshot();
    assert!(first.key_held(KeyCode::Space));
    assert!(first.key_pressed(KeyCode::Space));

    let second = input.take_snapshot();
    assert!(second.key_held(KeyCode::Space));
    assert!(!second.key_pressed(KeyCode::Space));

    input.keyboard_input(
        PhysicalKey::Code(KeyCode::Space),
        ElementState::Released,
        false,
    );
    let third = input.take_snapshot();
    assert!(!third.key_held(KeyCode::Space));
    assert!(third.key_released(KeyCode::Space));
}

#[test]
fn raw_motion_requires_focused_gameplay_capture() {
    let mut input = InputState::default();
    input.raw_mouse_motion(Vec2::ONE);
    input.set_focused(true);
    input.raw_mouse_motion(Vec2::splat(2.0));
    input.set_context(InputContext::GameplayCaptured);
    input.raw_mouse_motion(Vec2::new(3.0, -4.0));

    let snapshot = input.take_snapshot();
    assert_vector_close(snapshot.relative_mouse_motion(), Vec2::new(3.0, -4.0));
}

#[test]
fn scroll_units_and_cursor_state_remain_distinct() {
    let mut input = InputState::default();
    input.set_focused(true);
    input.cursor_entered();
    input.cursor_moved(Vec2::new(40.0, 60.0));
    input.mouse_input(MouseButton::Right, ElementState::Pressed);
    input.mouse_wheel(MouseScrollDelta::LineDelta(1.5, -2.0));
    input.mouse_wheel(MouseScrollDelta::PixelDelta(PhysicalPosition::new(
        8.0, -12.0,
    )));

    let snapshot = input.take_snapshot();
    assert!(snapshot.mouse_button_pressed(MouseButton::Right));
    assert_vector_close(snapshot.scroll_lines(), Vec2::new(1.5, -2.0));
    assert_vector_close(snapshot.scroll_pixels(), Vec2::new(8.0, -12.0));
    assert!(input.cursor_inside());
    assert_eq!(input.cursor_position(), Some(Vec2::new(40.0, 60.0)));

    input.cursor_left();
    assert!(!input.cursor_inside());
    assert_eq!(input.cursor_position(), None);
}

fn assert_vector_close(actual: Vec2, expected: Vec2) {
    assert!(actual.abs_diff_eq(expected, f32::EPSILON));
}
