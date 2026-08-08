//! Cross-platform process lifecycle primitives for Blackflower executables.
//!
//! Daemonization must happen before an async runtime, logger, or worker starts.
//! Unix targets detach the current process; Windows targets launch a detached
//! copy of the current executable and let the original launcher return. The
//! crate also owns terminal validation, operating-system shutdown signals, and
//! a small shared shutdown token used at executable boundaries.

use std::io::IsTerminal as _;
use std::sync::Arc;
use std::sync::atomic::{AtomicBool, Ordering};

/// Requested process launch mode.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum LaunchMode {
    /// Keep the current process attached to its interactive terminal.
    Foreground,
    /// Detach the runtime process from the launching terminal.
    Daemon,
}

impl LaunchMode {
    /// Select the launch mode represented by a `--foreground` flag.
    #[must_use]
    pub const fn from_foreground_flag(foreground: bool) -> Self {
        if foreground {
            Self::Foreground
        } else {
            Self::Daemon
        }
    }

    /// Enter the selected process mode before any runtime threads are started.
    pub fn enter(self) -> Result<LaunchOutcome, LaunchError> {
        match self {
            Self::Foreground => Ok(LaunchOutcome::Run),
            Self::Daemon => enter_daemon_mode(),
        }
    }
}

/// Role of the current process after applying a [`LaunchMode`].
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum LaunchOutcome {
    /// Continue into application initialization in this process.
    Run,
    /// Return successfully from the short-lived launcher process.
    ExitLauncher,
}

impl LaunchOutcome {
    /// Return whether this process owns application initialization.
    #[must_use]
    pub const fn should_run(self) -> bool {
        matches!(self, Self::Run)
    }
}

/// Cloneable cooperative shutdown request shared by process-owned workers.
///
/// This is intentionally a level-triggered flag: once shutdown is requested,
/// every current and future clone observes that request.
#[derive(Debug, Clone, Default)]
pub struct ShutdownToken {
    requested: Arc<AtomicBool>,
}

impl ShutdownToken {
    /// Construct a token whose shutdown request is initially clear.
    #[must_use]
    pub fn new() -> Self {
        Self::default()
    }

    /// Request cooperative process shutdown.
    pub fn request(&self) {
        self.requested.store(true, Ordering::Release);
    }

    /// Return whether shutdown has been requested.
    #[must_use]
    pub fn is_requested(&self) -> bool {
        self.requested.load(Ordering::Acquire)
    }

    /// Clone the underlying flag for lower-level loops that have not adopted
    /// [`ShutdownToken`] yet.
    #[must_use]
    pub fn shared_flag(&self) -> Arc<AtomicBool> {
        Arc::clone(&self.requested)
    }
}

/// Validate the interactive terminal required by a `--foreground` process.
///
/// Detached/background modes do not require standard input or output to be a
/// terminal.
pub fn validate_foreground_terminal(foreground: bool) -> Result<(), TerminalError> {
    validate_terminal_state(
        foreground,
        std::io::stdin().is_terminal(),
        std::io::stdout().is_terminal(),
    )
}

fn validate_terminal_state(
    foreground: bool,
    stdin_is_terminal: bool,
    stdout_is_terminal: bool,
) -> Result<(), TerminalError> {
    if foreground && (!stdin_is_terminal || !stdout_is_terminal) {
        Err(TerminalError::NotInteractive)
    } else {
        Ok(())
    }
}

/// Failure while validating the terminal contract of foreground mode.
#[derive(Debug, thiserror::Error)]
pub enum TerminalError {
    /// One or both standard streams are not attached to a terminal.
    #[error("--foreground requires an interactive terminal")]
    NotInteractive,
}

/// Operating-system event that requested process shutdown.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum ShutdownSignal {
    /// Console interrupt, normally Ctrl-C or SIGINT.
    Interrupt,
    /// Service termination request, normally SIGTERM.
    #[cfg(unix)]
    Terminate,
}

/// Wait for the platform's normal graceful-shutdown signal.
pub async fn wait_for_shutdown_signal() -> Result<ShutdownSignal, ShutdownSignalError> {
    #[cfg(unix)]
    {
        let mut terminate =
            tokio::signal::unix::signal(tokio::signal::unix::SignalKind::terminate())
                .map_err(ShutdownSignalError::InstallTerminate)?;
        tokio::select! {
            result = tokio::signal::ctrl_c() => {
                result
                    .map(|()| ShutdownSignal::Interrupt)
                    .map_err(ShutdownSignalError::Interrupt)
            }
            signal = terminate.recv() => {
                signal
                    .map(|()| ShutdownSignal::Terminate)
                    .ok_or(ShutdownSignalError::TerminateClosed)
            }
        }
    }

    #[cfg(not(unix))]
    {
        tokio::signal::ctrl_c()
            .await
            .map(|()| ShutdownSignal::Interrupt)
            .map_err(ShutdownSignalError::Interrupt)
    }
}

/// Failure while installing or waiting for an operating-system shutdown signal.
#[derive(Debug, thiserror::Error)]
pub enum ShutdownSignalError {
    /// The SIGTERM listener could not be installed.
    #[cfg(unix)]
    #[error("failed to install SIGTERM handler")]
    InstallTerminate(#[source] std::io::Error),
    /// The SIGINT/Ctrl-C listener failed.
    #[error("failed to wait for SIGINT")]
    Interrupt(#[source] std::io::Error),
    /// The installed SIGTERM stream closed without delivering a signal.
    #[cfg(unix)]
    #[error("SIGTERM signal stream closed")]
    TerminateClosed,
}

/// Failure while entering daemon mode.
#[derive(Debug, thiserror::Error)]
pub enum LaunchError {
    /// The Unix daemon must preserve the caller's working directory because
    /// command-line paths may be relative to it.
    #[cfg(unix)]
    #[error("failed to resolve the daemon working directory")]
    WorkingDirectory(#[source] std::io::Error),
    /// The Unix process could not fork, detach its session, or redirect stdio.
    #[cfg(unix)]
    #[error("failed to detach the Unix daemon process")]
    Unix(#[source] daemonize::Error),
    /// The intermediate Unix child could not complete the detach sequence.
    #[cfg(unix)]
    #[error("Unix daemon bootstrap exited with status {status}")]
    UnixBootstrap {
        /// Raw status reported by the intermediate daemonization child.
        status: i32,
    },
    /// The Windows launcher could not resolve the executable to relaunch.
    #[cfg(windows)]
    #[error("failed to resolve the daemon executable")]
    CurrentExecutable(#[source] std::io::Error),
    /// The Windows launcher could not start its detached runtime process.
    #[cfg(windows)]
    #[error("failed to launch the detached Windows daemon process")]
    WindowsSpawn(#[source] std::io::Error),
    /// The target has no supported daemon process model.
    #[cfg(not(any(unix, windows)))]
    #[error("daemon mode is unsupported on this target")]
    Unsupported,
}

#[cfg(unix)]
fn enter_daemon_mode() -> Result<LaunchOutcome, LaunchError> {
    let working_directory = std::env::current_dir().map_err(LaunchError::WorkingDirectory)?;
    let outcome = daemonize::Daemonize::new()
        .working_directory(working_directory)
        .execute();
    match outcome {
        daemonize::Outcome::Parent(Ok(parent)) if parent.first_child_exit_code == 0 => {
            Ok(LaunchOutcome::ExitLauncher)
        }
        daemonize::Outcome::Parent(Ok(parent)) => Err(LaunchError::UnixBootstrap {
            status: parent.first_child_exit_code,
        }),
        daemonize::Outcome::Parent(Err(error)) | daemonize::Outcome::Child(Err(error)) => {
            Err(LaunchError::Unix(error))
        }
        daemonize::Outcome::Child(Ok(_child)) => Ok(LaunchOutcome::Run),
    }
}

#[cfg(windows)]
fn enter_daemon_mode() -> Result<LaunchOutcome, LaunchError> {
    use std::ffi::OsStr;
    use std::os::windows::process::CommandExt as _;
    use std::process::{Command, Stdio};

    use windows_sys::Win32::System::Threading::DETACHED_PROCESS;

    const CHILD_MARKER: &str = "BLACKFLOWER_INTERNAL_DAEMON_CHILD";
    const CHILD_MARKER_VALUE: &str = "1";

    if std::env::var_os(CHILD_MARKER).as_deref() == Some(OsStr::new(CHILD_MARKER_VALUE)) {
        return Ok(LaunchOutcome::Run);
    }

    let executable = std::env::current_exe().map_err(LaunchError::CurrentExecutable)?;
    let _daemon = Command::new(executable)
        .args(std::env::args_os().skip(1))
        .env(CHILD_MARKER, CHILD_MARKER_VALUE)
        .stdin(Stdio::null())
        .stdout(Stdio::null())
        .stderr(Stdio::null())
        .creation_flags(DETACHED_PROCESS)
        .spawn()
        .map_err(LaunchError::WindowsSpawn)?;
    Ok(LaunchOutcome::ExitLauncher)
}

#[cfg(not(any(unix, windows)))]
fn enter_daemon_mode() -> Result<LaunchOutcome, LaunchError> {
    Err(LaunchError::Unsupported)
}

#[cfg(test)]
#[path = "../tests/unit/lib.rs"]
mod tests;
