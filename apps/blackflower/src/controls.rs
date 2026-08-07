use std::f64::consts::{FRAC_PI_2, TAU};

use blackflower_harness::ControlSubmission;
use blackflower_networking::{MAX_FUTURE_COMMAND_TICKS, SimulationTick};
use blackflower_networking_protocol::v1::MovementControl;
use winit::keyboard::KeyCode;

use crate::input::{InputContext, InputSnapshot};

const CONTROL_TICKS: u64 = 4;
const INITIAL_CONTROL_LEAD_TICKS: u64 = 4;
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
        input: &InputSnapshot,
    ) -> Result<Option<PreparedMovementControl>, ControlMappingError> {
        self.update_view(input);
        let Some((execute_tick, reset_timeline)) = self.schedule(current_tick)? else {
            return Ok(None);
        };
        let [move_right, move_forward] = movement_axes(input);
        let control = MovementControl::quantize(
            move_right,
            move_forward,
            self.view_yaw_radians,
            self.view_pitch_radians,
        )?;
        Ok(Some(PreparedMovementControl {
            submission: ControlSubmission {
                execute_tick,
                payload: control.encode().to_vec(),
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
    ) -> Result<Option<(SimulationTick, bool)>, ControlMappingError> {
        let maximum = current_tick
            .get()
            .checked_add(MAX_FUTURE_COMMAND_TICKS)
            .ok_or(ControlMappingError::TickOverflow)?;
        if let Some(next) = self.next_execute_tick
            && next > current_tick
        {
            return Ok((next.get() <= maximum).then_some((next, false)));
        }
        let first = align_up(
            current_tick
                .get()
                .checked_add(INITIAL_CONTROL_LEAD_TICKS)
                .ok_or(ControlMappingError::TickOverflow)?,
            CONTROL_TICKS,
        )?;
        Ok(Some((
            SimulationTick::new(first),
            self.next_execute_tick.is_some(),
        )))
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

fn movement_axes(input: &InputSnapshot) -> [f64; 2] {
    if !gameplay_active(input) {
        return [0.0; 2];
    }
    let mut right = axis(input, KeyCode::KeyD, KeyCode::KeyA);
    let mut forward = axis(input, KeyCode::KeyW, KeyCode::KeyS);
    let magnitude = right.hypot(forward);
    if magnitude > 1.0 {
        right /= magnitude;
        forward /= magnitude;
    }
    [right, forward]
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

#[cfg(test)]
#[path = "../tests/unit/controls.rs"]
mod tests;
