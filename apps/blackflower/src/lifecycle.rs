/// Process-facing state of the native client application.
#[derive(Debug, Default, Clone, Copy, PartialEq, Eq)]
pub enum ClientLifecycleState {
    /// The event loop has not delivered its first resume event.
    #[default]
    Cold,
    /// Platform resources may be used and the window is active.
    Active,
    /// The platform has temporarily suspended application resources.
    Suspended,
    /// Orderly shutdown has started.
    Stopping,
    /// The event loop has irreversibly exited.
    Exited,
}

/// Action required after a platform resume notification.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum ResumeAction {
    /// Construct the native window inside the active event loop.
    CreateWindow,
    /// Retain the window that survived suspension or a duplicate resume.
    RetainWindow,
    /// Ignore the notification because shutdown is already irreversible.
    Ignore,
}

/// Small idempotent state machine for platform and window lifetime.
#[derive(Debug, Default, Clone, Copy, PartialEq, Eq)]
pub struct ClientLifecycle {
    state: ClientLifecycleState,
    window_present: bool,
}

impl ClientLifecycle {
    /// Return the current process-facing lifecycle state.
    #[must_use]
    pub const fn state(self) -> ClientLifecycleState {
        self.state
    }

    /// Return whether the shell currently owns its native window.
    #[must_use]
    pub const fn window_present(self) -> bool {
        self.window_present
    }

    /// Handle one possibly redundant platform resume notification.
    pub fn resumed(&mut self) -> ResumeAction {
        match self.state {
            ClientLifecycleState::Cold
            | ClientLifecycleState::Active
            | ClientLifecycleState::Suspended => {
                self.state = ClientLifecycleState::Active;
                if self.window_present {
                    ResumeAction::RetainWindow
                } else {
                    ResumeAction::CreateWindow
                }
            }
            ClientLifecycleState::Stopping | ClientLifecycleState::Exited => ResumeAction::Ignore,
        }
    }

    /// Record successful native-window creation.
    pub fn window_created(&mut self) {
        self.window_present = true;
    }

    /// Record destruction of the native window.
    pub fn window_destroyed(&mut self) {
        self.window_present = false;
    }

    /// Handle one possibly redundant platform suspension notification.
    pub fn suspended(&mut self) {
        match self.state {
            ClientLifecycleState::Cold
            | ClientLifecycleState::Active
            | ClientLifecycleState::Suspended => {
                self.state = ClientLifecycleState::Suspended;
            }
            ClientLifecycleState::Stopping | ClientLifecycleState::Exited => {}
        }
    }

    /// Begin shutdown once and report whether this call changed the state.
    pub fn request_stop(&mut self) -> bool {
        match self.state {
            ClientLifecycleState::Cold
            | ClientLifecycleState::Active
            | ClientLifecycleState::Suspended => {
                self.state = ClientLifecycleState::Stopping;
                true
            }
            ClientLifecycleState::Stopping | ClientLifecycleState::Exited => false,
        }
    }

    /// Commit irreversible event-loop exit.
    pub fn exited(&mut self) {
        self.state = ClientLifecycleState::Exited;
        self.window_present = false;
    }
}

#[cfg(test)]
#[path = "../tests/unit/lifecycle.rs"]
mod tests;
