use std::error::Error as StdError;

use blackflower_networking::SimulationTick;
use blackflower_networking_protocol::v1::MovementControl;
use glam::DVec2;
use winit::event::ElementState;
use winit::keyboard::{KeyCode, PhysicalKey};

use crate::input::{InputContext, InputState};

use super::NativeMovementControls;

type TestResult = Result<(), Box<dyn StdError>>;

#[test]
fn gameplay_mapping_normalizes_diagonal_movement_and_updates_view() -> TestResult {
    let mut input = InputState::default();
    input.set_focused(true);
    input.set_context(InputContext::GameplayCaptured);
    press(&mut input, KeyCode::KeyW);
    press(&mut input, KeyCode::KeyD);
    input.raw_mouse_motion((20.0, -10.0));

    let mut controls = NativeMovementControls::default();
    let prepared = controls
        .prepare(SimulationTick::new(5), 12, &input.take_snapshot())?
        .ok_or("control was not scheduled")?;
    let control = MovementControl::decode(&prepared.submission.payload)?;

    assert_eq!(prepared.submission.execute_tick, SimulationTick::new(20));
    assert!(!prepared.reset_timeline);
    assert!((control.movement()[0] - std::f64::consts::FRAC_1_SQRT_2).abs() < 0.0001);
    assert!((control.movement()[1] - std::f64::consts::FRAC_1_SQRT_2).abs() < 0.0001);
    assert!(control.view_yaw().dequantize() > 6.0);
    assert!(control.view_pitch().dequantize() > 0.0);
    Ok(())
}

#[test]
fn user_interface_input_is_neutral_and_stalled_cadence_restarts() -> TestResult {
    let mut input = InputState::default();
    input.set_focused(true);
    press(&mut input, KeyCode::KeyW);

    let mut controls = NativeMovementControls::default();
    let first = controls
        .prepare(SimulationTick::new(8), 4, &input.take_snapshot())?
        .ok_or("first control was not scheduled")?;
    let first_tick = first.submission.execute_tick;
    let first_control = MovementControl::decode(&first.submission.payload)?;
    assert_vector_close(first_control.movement(), DVec2::ZERO);
    controls.commit(first_tick);

    let restarted = controls
        .prepare(SimulationTick::new(40), 4, &input.take_snapshot())?
        .ok_or("restart control was not scheduled")?;
    assert!(restarted.reset_timeline);
    assert_eq!(restarted.submission.execute_tick, SimulationTick::new(44));
    Ok(())
}

#[test]
fn increased_network_lead_rebases_the_consecutive_control_timeline() -> TestResult {
    let mut input = InputState::default();
    input.set_focused(true);
    input.set_context(InputContext::GameplayCaptured);

    let mut controls = NativeMovementControls::default();
    let first = controls
        .prepare(SimulationTick::new(8), 4, &input.take_snapshot())?
        .ok_or("first control was not scheduled")?;
    assert_eq!(first.submission.execute_tick, SimulationTick::new(12));
    controls.commit(first.submission.execute_tick);

    let rebased = controls
        .prepare(SimulationTick::new(9), 24, &input.take_snapshot())?
        .ok_or("rebased control was not scheduled")?;
    assert_eq!(rebased.submission.execute_tick, SimulationTick::new(28));
    assert!(rebased.reset_timeline);
    Ok(())
}

#[test]
fn input_lead_is_clamped_to_the_server_future_window() -> TestResult {
    let mut input = InputState::default();
    input.set_focused(true);
    input.set_context(InputContext::GameplayCaptured);

    let mut controls = NativeMovementControls::default();
    let prepared = controls
        .prepare(SimulationTick::new(5), 1_000, &input.take_snapshot())?
        .ok_or("bounded control was not scheduled")?;
    assert_eq!(prepared.submission.execute_tick, SimulationTick::new(28));
    Ok(())
}

fn press(input: &mut InputState, key: KeyCode) {
    input.keyboard_input(PhysicalKey::Code(key), ElementState::Pressed, false);
}

fn assert_vector_close(actual: DVec2, expected: DVec2) {
    assert!(actual.abs_diff_eq(expected, f64::EPSILON));
}
