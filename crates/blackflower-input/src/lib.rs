use std::f32::consts::FRAC_PI_2;
use std::sync::{Arc, Mutex};

use blackflower_protocol::Command;
use blackflower_time::Tick;

use crate::components::InputButtons;

pub mod components;

/// Pitch is clamped just shy of straight up/down so the horizontal facing used
/// for movement never collapses to zero.
const PITCH_LIMIT: f32 = FRAC_PI_2 - 0.01;

/// Mutable input state shared between the window thread (which mutates it) and
/// the tick thread (which samples it into a [`Command`]).
#[derive(Debug, Default, Clone, Copy)]
struct InputState {
    buttons: InputButtons,
    /// Absolute view angles in radians (yaw about +Y, pitch about +X).
    yaw: f32,
    pitch: f32,
}

#[derive(Debug, Default)]
pub struct InputHandle {
    inner: Arc<Mutex<InputState>>,
}

impl InputHandle {
    pub fn command(&self, tick: Tick) -> Command {
        let state = match self.inner.lock() {
            Ok(g) => *g,
            Err(poisoned) => *poisoned.into_inner(),
        };

        Command {
            tick: tick.as_u64(),
            buttons: state.buttons.bits(),
            yaw: state.yaw,
            pitch: state.pitch,
            snapshot_ack_tick: 0,
            snapshot_ack_bits: 0,
        }
    }

    pub fn press(&self, button: InputButtons) {
        if let Ok(mut g) = self.inner.lock() {
            g.buttons.insert(button);
        }
    }

    pub fn release(&self, button: InputButtons) {
        if let Ok(mut g) = self.inner.lock() {
            g.buttons.remove(button);
        }
    }

    /// Accumulate a relative mouse motion into the absolute view angles. `dyaw`
    /// and `dpitch` are pre-scaled by sensitivity (radians). Pitch is clamped;
    /// yaw wraps freely.
    pub fn look(&self, dyaw: f32, dpitch: f32) {
        if let Ok(mut g) = self.inner.lock() {
            g.yaw += dyaw;
            g.pitch = (g.pitch + dpitch).clamp(-PITCH_LIMIT, PITCH_LIMIT);
        }
    }

    /// Release all buttons (e.g. on focus loss). View angles are preserved.
    pub fn clear(&self) {
        if let Ok(mut g) = self.inner.lock() {
            g.buttons = InputButtons::empty();
        }
    }
}
