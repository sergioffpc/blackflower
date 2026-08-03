use super::{ClientLifecycle, ClientLifecycleState, ResumeAction};

#[test]
fn duplicate_resume_creates_only_one_window() {
    let mut lifecycle = ClientLifecycle::default();

    assert_eq!(lifecycle.resumed(), ResumeAction::CreateWindow);
    lifecycle.window_created();
    assert_eq!(lifecycle.resumed(), ResumeAction::RetainWindow);
    assert_eq!(lifecycle.state(), ClientLifecycleState::Active);
    assert!(lifecycle.window_present());
}

#[test]
fn suspension_retains_an_existing_window() {
    let mut lifecycle = ClientLifecycle::default();
    assert_eq!(lifecycle.resumed(), ResumeAction::CreateWindow);
    lifecycle.window_created();

    lifecycle.suspended();
    lifecycle.suspended();
    assert_eq!(lifecycle.state(), ClientLifecycleState::Suspended);
    assert_eq!(lifecycle.resumed(), ResumeAction::RetainWindow);
}

#[test]
fn destroyed_window_is_recreated_after_resume() {
    let mut lifecycle = ClientLifecycle::default();
    assert_eq!(lifecycle.resumed(), ResumeAction::CreateWindow);
    lifecycle.window_created();
    lifecycle.suspended();
    lifecycle.window_destroyed();

    assert_eq!(lifecycle.resumed(), ResumeAction::CreateWindow);
}

#[test]
fn shutdown_rejects_late_platform_transitions() {
    let mut lifecycle = ClientLifecycle::default();
    assert!(lifecycle.request_stop());
    assert!(!lifecycle.request_stop());
    lifecycle.suspended();
    assert_eq!(lifecycle.resumed(), ResumeAction::Ignore);
    assert_eq!(lifecycle.state(), ClientLifecycleState::Stopping);

    lifecycle.exited();
    assert_eq!(lifecycle.state(), ClientLifecycleState::Exited);
    assert!(!lifecycle.window_present());
}
