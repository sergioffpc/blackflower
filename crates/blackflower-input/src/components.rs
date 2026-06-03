use bitflags::bitflags;
use blackflower_math::Vec2;
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

impl InputButtons {
    /// Compute the normalized 2D movement direction in the XZ plane.
    ///
    /// Convention: `+x` right, `-z` forward (matching the renderer's
    /// camera setup). Returns `Vec2::ZERO` when no buttons are active or
    /// when opposing buttons cancel out.
    ///
    /// The result is normalized so diagonal movement is not faster than
    /// axis-aligned movement.
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
