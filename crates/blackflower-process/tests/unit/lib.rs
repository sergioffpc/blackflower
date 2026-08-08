use super::{LaunchMode, LaunchOutcome, ShutdownToken, TerminalError, validate_terminal_state};

#[test]
fn foreground_flag_selects_the_process_mode() {
    assert_eq!(
        LaunchMode::from_foreground_flag(true),
        LaunchMode::Foreground
    );
    assert_eq!(LaunchMode::from_foreground_flag(false), LaunchMode::Daemon);
}

#[test]
fn launch_outcome_identifies_the_runtime_process() {
    assert!(LaunchOutcome::Run.should_run());
    assert!(!LaunchOutcome::ExitLauncher.should_run());
}

#[test]
fn shutdown_request_is_shared_and_level_triggered() {
    let token = ShutdownToken::new();
    let observer = token.clone();

    assert!(!observer.is_requested());
    token.request();
    assert!(observer.is_requested());
}

#[test]
fn foreground_requires_both_terminal_streams() {
    assert!(validate_terminal_state(true, true, true).is_ok());
    assert!(matches!(
        validate_terminal_state(true, false, true),
        Err(TerminalError::NotInteractive)
    ));
    assert!(matches!(
        validate_terminal_state(true, true, false),
        Err(TerminalError::NotInteractive)
    ));
}

#[test]
fn background_mode_does_not_require_a_terminal() {
    assert!(validate_terminal_state(false, false, false).is_ok());
}
