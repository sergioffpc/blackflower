use super::{Status, check, raw};

#[test]
fn configuration_mismatch_status_is_preserved() {
    assert_eq!(
        check(raw::BF_PHYSICS_STATUS_CONFIGURATION_MISMATCH.cast_signed()),
        Err(Status::ConfigurationMismatch),
    );
}
