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

use bitflags::bitflags;
use serde::{Deserialize, Serialize};

bitflags! {
    /// Buttons currently pressed by the player.
    ///
    /// Each variant corresponds to one digital input. Multiple may be set
    /// simultaneously (e.g. `FORWARD | RIGHT` for diagonal movement).
    /// Encoded over the wire as a single `u8`.
    #[derive(Copy, Clone, Debug, Default, PartialEq, Eq, Serialize, Deserialize)]
    pub struct InputButtons: u8 {
        const FORWARD  = 1 << 0;
        const BACKWARD = 1 << 1;
        const LEFT     = 1 << 2;
        const RIGHT    = 1 << 3;
    }
}

#[derive(Clone, Debug, Default)]
pub struct InputHandle {
    inner: Arc<Mutex<InputButtons>>,
}

impl InputHandle {
    pub fn snapshot(&self) -> InputButtons {
        match self.inner.lock() {
            Ok(g) => *g,
            Err(poisoned) => *poisoned.into_inner(),
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
