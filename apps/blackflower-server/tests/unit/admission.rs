use std::time::Duration;

use blackflower_networking::{AuthorityError, ConnectionEpoch, ProtocolRevision, SessionAuthority};

use super::{CompatibilityContract, LoopbackSessionAuthority};

type TestResult = Result<(), Box<dyn std::error::Error>>;

#[test]
fn reissued_resume_credentials_do_not_revalidate_consumed_bytes() -> TestResult {
    let contract = CompatibilityContract {
        protocol_revision: ProtocolRevision::V1,
    };
    let mut authority = LoopbackSessionAuthority::new(contract);
    let claims = authority.admit(Duration::ZERO)?;
    let first = authority.issue_resume(&claims, Duration::ZERO)?;
    let _resumed = authority.consume_resume(
        &first.token,
        ConnectionEpoch::new(2),
        Duration::from_secs(1),
    )?;
    let second = authority.issue_resume(&claims, Duration::from_secs(1))?;

    assert_ne!(first.token, second.token);
    assert_eq!(
        authority.consume_resume(
            &first.token,
            ConnectionEpoch::new(3),
            Duration::from_secs(2),
        ),
        Err(AuthorityError::Invalid)
    );
    assert!(
        authority
            .consume_resume(
                &second.token,
                ConnectionEpoch::new(3),
                Duration::from_secs(2),
            )
            .is_ok()
    );
    Ok(())
}
