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
    input.raw_mouse_motion((3.0, -2.0));
    input.modifiers_changed(ModifiersState::SHIFT);

    input.set_focused(false);
    let snapshot = input.take_snapshot();

    assert!(!snapshot.focused());
    assert_eq!(snapshot.context(), InputContext::UserInterface);
    assert!(!snapshot.key_held(KeyCode::KeyW));
    assert!(!snapshot.mouse_button_held(MouseButton::Left));
    assert_eq!(snapshot.modifiers(), ModifiersState::empty());
    assert_pair_close(snapshot.relative_mouse_motion(), (0.0, 0.0));
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
    input.raw_mouse_motion((1.0, 1.0));
    input.set_focused(true);
    input.raw_mouse_motion((2.0, 2.0));
    input.set_context(InputContext::GameplayCaptured);
    input.raw_mouse_motion((3.0, -4.0));

    let snapshot = input.take_snapshot();
    assert_pair_close(snapshot.relative_mouse_motion(), (3.0, -4.0));
}

#[test]
fn scroll_units_and_cursor_state_remain_distinct() {
    let mut input = InputState::default();
    input.set_focused(true);
    input.cursor_entered();
    input.cursor_moved((40.0, 60.0));
    input.mouse_input(MouseButton::Right, ElementState::Pressed);
    input.mouse_wheel(MouseScrollDelta::LineDelta(1.5, -2.0));
    input.mouse_wheel(MouseScrollDelta::PixelDelta(PhysicalPosition::new(
        8.0, -12.0,
    )));

    let snapshot = input.take_snapshot();
    assert!(snapshot.mouse_button_pressed(MouseButton::Right));
    assert_pair_close_f32(snapshot.scroll_lines(), (1.5, -2.0));
    assert_pair_close(snapshot.scroll_pixels(), (8.0, -12.0));
    assert!(input.cursor_inside());
    assert_eq!(input.cursor_position(), Some((40.0, 60.0)));

    input.cursor_left();
    assert!(!input.cursor_inside());
    assert_eq!(input.cursor_position(), None);
}

fn assert_pair_close(actual: (f64, f64), expected: (f64, f64)) {
    assert!((actual.0 - expected.0).abs() < f64::EPSILON);
    assert!((actual.1 - expected.1).abs() < f64::EPSILON);
}

fn assert_pair_close_f32(actual: (f32, f32), expected: (f32, f32)) {
    assert!((actual.0 - expected.0).abs() < f32::EPSILON);
    assert!((actual.1 - expected.1).abs() < f32::EPSILON);
}
