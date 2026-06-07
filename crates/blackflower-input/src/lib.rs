//! Player input primitives.
//!
//! Defines [`InputButtons`], a bitfield encoding which digital actions are
//! currently active. The same type is used:
//!
//! - On the client, to capture and serialize input over the network.
//! - On the server, to apply incoming commands authoritatively.
//! - In the shared simulation system, to compute movement.
//!
//! Encoded over the wire as a single `u8`.

use std::sync::{Arc, Mutex};

use blackflower_protocol::Command;
use blackflower_tick::Tick;

use crate::components::InputButtons;

pub mod components;

#[derive(Debug, Default)]
pub struct InputHandle {
    inner: Arc<Mutex<InputButtons>>,
}

impl InputHandle {
    pub fn command(&self, tick: Tick) -> Command {
        let buttons = match self.inner.lock() {
            Ok(g) => *g,
            Err(poisoned) => *poisoned.into_inner(),
        };

        Command {
            tick: tick.into(),
            buttons: buttons.bits(),
        }
    }

    pub fn press(&self, button: InputButtons) {
        if let Ok(mut g) = self.inner.lock() {
            g.insert(button);
        }
    }

    pub fn release(&self, button: InputButtons) {
        if let Ok(mut g) = self.inner.lock() {
            g.remove(button);
        }
    }

    pub fn clear(&self) {
        if let Ok(mut g) = self.inner.lock() {
            *g = InputButtons::empty();
        }
    }
}
