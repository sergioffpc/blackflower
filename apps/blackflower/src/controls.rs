use std::f64::consts::{FRAC_PI_2, TAU};

use blackflower_harness::ControlSubmission;
use blackflower_networking::{
    INITIAL_INPUT_LEAD_TICKS, INPUT_GRACE_TICKS, MAX_FUTURE_COMMAND_TICKS, SimulationTick,
};
use blackflower_networking_protocol::v1::MovementControl;
use bytes::Bytes;
use glam::DVec2;
use winit::keyboard::KeyCode;

use crate::input::{InputContext, InputSnapshot};

const CONTROL_TICKS: u64 = 4;
const MOUSE_RADIANS_PER_UNIT: f64 = 0.0025;

/// One prepared canonical control whose scheduler state is committed after submission.
pub(crate) struct PreparedMovementControl {
    pub(crate) submission: ControlSubmission,
    pub(crate) reset_timeline: bool,
}

/// Stateful native-input mapper and consecutive 60 Hz control scheduler.
#[derive(Debug, Default)]
pub(crate) struct NativeMovementControls {
    view_yaw_radians: f64,
    view_pitch_radians: f64,
    next_execute_tick: Option<SimulationTick>,
}

impl NativeMovementControls {
    /// Map one device sample when the next control fits the bounded future window.
    pub(crate) fn prepare(
        &mut self,
        current_tick: SimulationTick,
        input_lead_ticks: u64,
        input: &InputSnapshot,
    ) -> Result<Option<PreparedMovementControl>, ControlMappingError> {
        self.update_view(input);
        let Some((execute_tick, reset_timeline)) = self.schedule(current_tick, input_lead_ticks)?
        else {
            return Ok(None);
        };
        let control = MovementControl::quantize(
            movement_axes(input),
            self.view_yaw_radians,
            self.view_pitch_radians,
        )?;
        Ok(Some(PreparedMovementControl {
            submission: ControlSubmission {
                execute_tick,
                payload: Bytes::copy_from_slice(&control.encode()),
                commands: Vec::new(),
            },
            reset_timeline,
        }))
    }

    /// Commit a successfully submitted execution tick.
    pub(crate) fn commit(&mut self, execute_tick: SimulationTick) {
        self.next_execute_tick = execute_tick
            .get()
            .checked_add(CONTROL_TICKS)
            .map(SimulationTick::new);
    }

    /// Forget cadence when prediction is not on an active session timeline.
    pub(crate) fn reset(&mut self) {
        self.next_execute_tick = None;
    }

    fn update_view(&mut self, input: &InputSnapshot) {
        if !gameplay_active(input) {
            return;
        }
        let (horizontal, vertical) = input.relative_mouse_motion();
        self.view_yaw_radians =
            (self.view_yaw_radians - horizontal * MOUSE_RADIANS_PER_UNIT).rem_euclid(TAU);
        self.view_pitch_radians = (self.view_pitch_radians - vertical * MOUSE_RADIANS_PER_UNIT)
            .clamp(-FRAC_PI_2, FRAC_PI_2);
    }

    fn schedule(
        &self,
        current_tick: SimulationTick,
        input_lead_ticks: u64,
    ) -> Result<Option<(SimulationTick, bool)>, ControlMappingError> {
        let maximum = current_tick
            .get()
            .checked_add(MAX_FUTURE_COMMAND_TICKS)
            .ok_or(ControlMappingError::TickOverflow)?;
        let desired = first_execution_tick(current_tick, input_lead_ticks, maximum)?;
        if let Some(next) = self.next_execute_tick
            && next > current_tick
        {
            if next < desired {
                let grace_bounded = desired
                    .get()
                    .min(next.get().saturating_add(INPUT_GRACE_TICKS));
                return Ok(Some((SimulationTick::new(grace_bounded), true)));
            }
            return Ok((next.get() <= maximum).then_some((next, false)));
        }
        Ok(Some((desired, self.next_execute_tick.is_some())))
    }
}

/// Native input could not be represented as a canonical v1 movement control.
#[derive(Debug, thiserror::Error)]
pub(crate) enum ControlMappingError {
    /// Tick scheduling exhausted the network representation.
    #[error("movement control tick overflow")]
    TickOverflow,
    /// Canonical v1 movement quantization rejected the mapped input.
    #[error(transparent)]
    Protocol(#[from] blackflower_networking_protocol::v1::ProtocolError),
}

fn movement_axes(input: &InputSnapshot) -> DVec2 {
    if !gameplay_active(input) {
        return DVec2::ZERO;
    }
    DVec2::new(
        axis(input, KeyCode::KeyD, KeyCode::KeyA),
        axis(input, KeyCode::KeyW, KeyCode::KeyS),
    )
    .clamp_length_max(1.0)
}

fn axis(input: &InputSnapshot, positive: KeyCode, negative: KeyCode) -> f64 {
    f64::from(u8::from(input.key_held(positive))) - f64::from(u8::from(input.key_held(negative)))
}

fn gameplay_active(input: &InputSnapshot) -> bool {
    input.focused() && input.context() == InputContext::GameplayCaptured
}

fn align_up(value: u64, quantum: u64) -> Result<u64, ControlMappingError> {
    value
        .checked_add(quantum - 1)
        .map(|rounded| rounded / quantum * quantum)
        .ok_or(ControlMappingError::TickOverflow)
}

fn first_execution_tick(
    current_tick: SimulationTick,
    input_lead_ticks: u64,
    maximum: u64,
) -> Result<SimulationTick, ControlMappingError> {
    let lead = input_lead_ticks.clamp(CONTROL_TICKS, MAX_FUTURE_COMMAND_TICKS);
    let requested = current_tick
        .get()
        .checked_add(lead)
        .ok_or(ControlMappingError::TickOverflow)?;
    let aligned = align_up(requested, CONTROL_TICKS)?;
    let bounded = if aligned > maximum {
        maximum / CONTROL_TICKS * CONTROL_TICKS
    } else {
        aligned
    };
    Ok(SimulationTick::new(bounded))
}

const _: () = assert!(INITIAL_INPUT_LEAD_TICKS.is_multiple_of(CONTROL_TICKS));

#[cfg(test)]
#[path = "../tests/unit/controls.rs"]
mod tests;
