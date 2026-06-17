use std::sync::{Arc, Mutex};

use blackflower_protocol::Command;
use blackflower_time::Tick;

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
            tick: tick.as_u64(),
            buttons: buttons.bits(),
            snapshot_ack_tick: 0,
            snapshot_ack_bits: 0,
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
