use std::collections::BTreeSet;
use std::mem;

use glam::Vec2;
use winit::event::{ElementState, MouseButton, MouseScrollDelta};
use winit::keyboard::{KeyCode, ModifiersState, PhysicalKey};

/// Active consumer of native keyboard and pointing-device state.
#[derive(Debug, Default, Clone, Copy, PartialEq, Eq)]
pub enum InputContext {
    /// Pointer movement and text-oriented input remain under desktop control.
    #[default]
    UserInterface,
    /// Relative pointing-device motion may drive gameplay controls.
    GameplayCaptured,
}

/// One source-neutral snapshot of physical input collected since the last sample.
#[derive(Debug, Clone)]
pub struct InputSnapshot {
    context: InputContext,
    focused: bool,
    modifiers: ModifiersState,
    held_keys: BTreeSet<KeyCode>,
    pressed_keys: BTreeSet<KeyCode>,
    released_keys: BTreeSet<KeyCode>,
    held_mouse_buttons: BTreeSet<MouseButton>,
    pressed_mouse_buttons: BTreeSet<MouseButton>,
    released_mouse_buttons: BTreeSet<MouseButton>,
    relative_mouse_motion: Vec2,
    scroll_lines: Vec2,
    scroll_pixels: Vec2,
}

impl InputSnapshot {
    /// Return the active input-routing context.
    #[must_use]
    pub const fn context(&self) -> InputContext {
        self.context
    }

    /// Return whether the native window was focused at sample time.
    #[must_use]
    pub const fn focused(&self) -> bool {
        self.focused
    }

    /// Return the active modifier bitset.
    #[must_use]
    pub const fn modifiers(&self) -> ModifiersState {
        self.modifiers
    }

    /// Return whether a physical key is currently held.
    #[must_use]
    pub fn key_held(&self, key: KeyCode) -> bool {
        self.held_keys.contains(&key)
    }

    /// Return whether a physical key gained its edge in this sample.
    #[must_use]
    pub fn key_pressed(&self, key: KeyCode) -> bool {
        self.pressed_keys.contains(&key)
    }

    /// Return whether a physical key lost its edge in this sample.
    #[must_use]
    pub fn key_released(&self, key: KeyCode) -> bool {
        self.released_keys.contains(&key)
    }

    /// Return whether a pointing-device button is currently held.
    #[must_use]
    pub fn mouse_button_held(&self, button: MouseButton) -> bool {
        self.held_mouse_buttons.contains(&button)
    }

    /// Return whether a pointing-device button gained its edge in this sample.
    #[must_use]
    pub fn mouse_button_pressed(&self, button: MouseButton) -> bool {
        self.pressed_mouse_buttons.contains(&button)
    }

    /// Return whether a pointing-device button lost its edge in this sample.
    #[must_use]
    pub fn mouse_button_released(&self, button: MouseButton) -> bool {
        self.released_mouse_buttons.contains(&button)
    }

    /// Return accumulated raw pointing-device motion.
    #[must_use]
    pub const fn relative_mouse_motion(&self) -> Vec2 {
        self.relative_mouse_motion
    }

    /// Return accumulated line-oriented scroll motion.
    #[must_use]
    pub const fn scroll_lines(&self) -> Vec2 {
        self.scroll_lines
    }

    /// Return accumulated pixel-oriented scroll motion.
    #[must_use]
    pub const fn scroll_pixels(&self) -> Vec2 {
        self.scroll_pixels
    }
}

/// Reducer for native device events before action mapping or canonical encoding.
#[derive(Debug, Default)]
pub struct InputState {
    context: InputContext,
    focused: bool,
    cursor_inside: bool,
    cursor_position: Option<Vec2>,
    modifiers: ModifiersState,
    held_keys: BTreeSet<KeyCode>,
    pressed_keys: BTreeSet<KeyCode>,
    released_keys: BTreeSet<KeyCode>,
    held_mouse_buttons: BTreeSet<MouseButton>,
    pressed_mouse_buttons: BTreeSet<MouseButton>,
    released_mouse_buttons: BTreeSet<MouseButton>,
    relative_mouse_motion: Vec2,
    scroll_lines: Vec2,
    scroll_pixels: Vec2,
}

impl InputState {
    /// Return the active input-routing context.
    #[must_use]
    pub const fn context(&self) -> InputContext {
        self.context
    }

    /// Return whether the native window currently has focus.
    #[must_use]
    pub const fn focused(&self) -> bool {
        self.focused
    }

    /// Return whether the desktop cursor lies inside the native window.
    #[must_use]
    pub const fn cursor_inside(&self) -> bool {
        self.cursor_inside
    }

    /// Return the latest window-relative cursor position.
    #[must_use]
    pub const fn cursor_position(&self) -> Option<Vec2> {
        self.cursor_position
    }

    /// Change the active consumer and neutralize state crossing the boundary.
    pub fn set_context(&mut self, context: InputContext) {
        if self.context != context {
            self.context = context;
            self.reset_devices();
        }
    }

    /// Apply one focus transition, neutralizing every device on focus loss.
    pub fn set_focused(&mut self, focused: bool) {
        self.focused = focused;
        if !focused {
            self.cursor_inside = false;
            self.cursor_position = None;
            self.context = InputContext::UserInterface;
            self.reset_devices();
        }
    }

    /// Release every device when the platform suspends the application.
    pub fn suspend(&mut self) {
        self.set_focused(false);
    }

    /// Apply one physical keyboard edge.
    pub fn keyboard_input(
        &mut self,
        physical_key: PhysicalKey,
        state: ElementState,
        synthetic: bool,
    ) {
        let PhysicalKey::Code(key) = physical_key else {
            return;
        };
        if !self.focused || synthetic && state == ElementState::Pressed {
            return;
        }
        update_edge_set(
            key,
            state,
            &mut self.held_keys,
            &mut self.pressed_keys,
            &mut self.released_keys,
        );
    }

    /// Replace the active keyboard modifier bitset.
    pub fn modifiers_changed(&mut self, modifiers: ModifiersState) {
        if self.focused {
            self.modifiers = modifiers;
        }
    }

    /// Apply one physical pointing-device button edge.
    pub fn mouse_input(&mut self, button: MouseButton, state: ElementState) {
        if !self.focused {
            return;
        }
        update_edge_set(
            button,
            state,
            &mut self.held_mouse_buttons,
            &mut self.pressed_mouse_buttons,
            &mut self.released_mouse_buttons,
        );
    }

    /// Accumulate raw motion only while gameplay owns the focused pointer.
    pub fn raw_mouse_motion(&mut self, delta: Vec2) {
        if self.focused && self.context == InputContext::GameplayCaptured {
            self.relative_mouse_motion += delta;
        }
    }

    /// Record a desktop-cursor position for user-interface hit testing.
    pub fn cursor_moved(&mut self, position: Vec2) {
        if self.focused {
            self.cursor_inside = true;
            self.cursor_position = Some(position);
        }
    }

    /// Record that the desktop cursor entered the window.
    pub fn cursor_entered(&mut self) {
        if self.focused {
            self.cursor_inside = true;
        }
    }

    /// Record that the desktop cursor left the window.
    pub fn cursor_left(&mut self) {
        self.cursor_inside = false;
        self.cursor_position = None;
    }

    /// Accumulate scroll motion while preserving its native unit.
    pub fn mouse_wheel(&mut self, delta: MouseScrollDelta) {
        if !self.focused {
            return;
        }
        match delta {
            MouseScrollDelta::LineDelta(x, y) => {
                self.scroll_lines += Vec2::new(x, y);
            }
            MouseScrollDelta::PixelDelta(position) => {
                self.scroll_pixels += vec2_from_f64(position.x, position.y);
            }
        }
    }

    /// Snapshot current holds and consume all accumulated edges and deltas.
    pub fn take_snapshot(&mut self) -> InputSnapshot {
        InputSnapshot {
            context: self.context,
            focused: self.focused,
            modifiers: self.modifiers,
            held_keys: self.held_keys.clone(),
            pressed_keys: mem::take(&mut self.pressed_keys),
            released_keys: mem::take(&mut self.released_keys),
            held_mouse_buttons: self.held_mouse_buttons.clone(),
            pressed_mouse_buttons: mem::take(&mut self.pressed_mouse_buttons),
            released_mouse_buttons: mem::take(&mut self.released_mouse_buttons),
            relative_mouse_motion: mem::take(&mut self.relative_mouse_motion),
            scroll_lines: mem::take(&mut self.scroll_lines),
            scroll_pixels: mem::take(&mut self.scroll_pixels),
        }
    }

    fn reset_devices(&mut self) {
        self.modifiers = ModifiersState::empty();
        self.held_keys.clear();
        self.pressed_keys.clear();
        self.released_keys.clear();
        self.held_mouse_buttons.clear();
        self.pressed_mouse_buttons.clear();
        self.released_mouse_buttons.clear();
        self.relative_mouse_motion = Vec2::ZERO;
        self.scroll_lines = Vec2::ZERO;
        self.scroll_pixels = Vec2::ZERO;
    }
}

#[allow(
    clippy::cast_possible_truncation,
    reason = "native pointing-device coordinates enter the canonical f32 input domain at this boundary"
)]
fn vec2_from_f64(x: f64, y: f64) -> Vec2 {
    Vec2::new(x as f32, y as f32)
}

fn update_edge_set<T>(
    value: T,
    state: ElementState,
    held: &mut BTreeSet<T>,
    pressed: &mut BTreeSet<T>,
    released: &mut BTreeSet<T>,
) where
    T: Copy + Ord,
{
    match state {
        ElementState::Pressed => {
            if held.insert(value) {
                pressed.insert(value);
            }
        }
        ElementState::Released => {
            if held.remove(&value) {
                released.insert(value);
            }
        }
    }
}

#[cfg(test)]
#[path = "../tests/unit/input.rs"]
mod tests;
