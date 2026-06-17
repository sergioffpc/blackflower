use bitflags::bitflags;
use blackflower_math::Vec2;
use serde::{Deserialize, Serialize};

bitflags! {
    #[derive(Copy, Clone, Debug, Default, PartialEq, Eq, Serialize, Deserialize)]
    pub struct InputButtons: u64 {
        const FORWARD  = 1 << 0;
        const BACKWARD = 1 << 1;
        const LEFT     = 1 << 2;
        const RIGHT    = 1 << 3;
    }
}

impl InputButtons {
    /// Resolve a binding action name (case-insensitive) to its flag, e.g.
    /// `"forward"` → [`InputButtons::FORWARD`]. Returns `None` for unknown names.
    #[must_use]
    pub fn from_action(name: &str) -> Option<Self> {
        Self::from_name(&name.to_uppercase())
    }

    #[must_use]
    pub fn normalize_or_zero(&self) -> Vec2 {
        let mut d = Vec2::ZERO;
        if self.contains(Self::FORWARD) {
            d.y -= 1.0;
        }
        if self.contains(Self::BACKWARD) {
            d.y += 1.0;
        }
        if self.contains(Self::LEFT) {
            d.x -= 1.0;
        }
        if self.contains(Self::RIGHT) {
            d.x += 1.0;
        }
        d.normalize_or_zero()
    }
}
