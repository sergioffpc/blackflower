use super::*;

#[test]
fn telemetry_labels_are_derived_without_changing_the_contract() {
    assert_eq!(MetricDirection::Upstream.as_str(), "upstream");
    assert_eq!(MetricDirection::Downstream.as_str(), "downstream");
    assert_eq!(InputAction::Submitted.as_str(), "submitted");
    assert_eq!(InputAction::Accepted.as_str(), "accepted");
    assert_eq!(SnapshotAction::Sent.as_str(), "sent");
    assert_eq!(SnapshotAction::Applied.as_str(), "applied");
    assert_eq!(SnapshotAction::Acknowledged.as_str(), "acknowledged");
    assert_eq!(ResyncAction::Requested.as_str(), "requested");
    assert_eq!(ResyncAction::Started.as_str(), "started");
    assert_eq!(ClockState::Synchronized.as_str(), "synchronized");
    assert_eq!(ClockState::Unsynchronized.as_str(), "unsynchronized");
    assert_eq!(QueueKind::Control.as_str(), "control");
    assert_eq!(QueueKind::Bootstrap.as_str(), "bootstrap");
    assert_eq!(QueueKind::Input.as_str(), "input");
    assert_eq!(QueueKind::Snapshot.as_str(), "snapshot");
    assert_eq!(QueueKind::Voice.as_str(), "voice");
    assert_eq!(QueueKind::Host.as_str(), "host");
    assert_eq!(DropReason::Superseded.as_str(), "superseded");
    assert_eq!(DropReason::Deadline.as_str(), "deadline");
    assert_eq!(DropReason::Budget.as_str(), "budget");
    assert_eq!(DropReason::QueueFull.as_str(), "queue_full");
    assert_eq!(DropReason::Late.as_str(), "late");
    assert_eq!(ViolationKind::Wire.as_str(), "wire");
    assert_eq!(ViolationKind::Session.as_str(), "session");
    assert_eq!(
        ViolationKind::ConflictingIdentity.as_str(),
        "conflicting_identity"
    );
    assert_eq!(ViolationKind::Compatibility.as_str(), "compatibility");
    assert_eq!(ViolationKind::Voice.as_str(), "voice");
}
