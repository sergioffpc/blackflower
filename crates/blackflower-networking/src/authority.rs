use std::time::Duration;

use crate::{
    AdmissionClaims, ConnectionEpoch, MAX_RESUME_TOKEN_BYTES, MatchId, PlayerId, SessionId,
    WireError,
};

/// Reconnect interval during which a one-use resume token may be consumed.
pub const RECONNECT_WINDOW: Duration = Duration::from_secs(30);

/// Claims recovered from an atomically consumed resume token.
#[derive(Debug, Clone, PartialEq, Eq)]
pub struct ResumeClaims {
    /// Resumed application session.
    pub session_id: SessionId,
    /// Resumed player identity.
    pub player_id: PlayerId,
    /// Match that still owns the authoritative state.
    pub match_id: MatchId,
    /// Fresh connection generation assigned to the replacement connection.
    pub connection_epoch: ConnectionEpoch,
}

/// Newly issued opaque one-use resume credential.
#[derive(Debug, Clone, PartialEq, Eq)]
pub struct IssuedResumeToken {
    /// Opaque credential understood only by the authority implementation.
    pub token: Vec<u8>,
    /// Absolute monotonic expiry instant in the authority's time domain.
    pub expires_at: Duration,
}

/// Failure returned by the external admission and resume authority.
#[derive(Debug, Clone, PartialEq, Eq, thiserror::Error)]
pub enum AuthorityError {
    /// Credential signature or claims are invalid.
    #[error("invalid session credential")]
    Invalid,
    /// Credential is outside its permitted lifetime.
    #[error("session credential expired")]
    Expired,
    /// A one-use credential was already atomically consumed.
    #[error("session credential was replayed")]
    Replayed,
    /// Credential exceeds the protocol-level opaque size bound.
    #[error(transparent)]
    Wire(#[from] WireError),
    /// Backing authority is temporarily unavailable.
    #[error("session authority is unavailable")]
    Unavailable,
}

/// External authority boundary for session identity and reconnect credentials.
///
/// Ordinary admission is deliberately credential-free until authentication and
/// matchmaking are composed. Resume-token consumption remains atomic.
pub trait SessionAuthority {
    /// Assign identities to one protocol-compatible connection.
    fn admit(&mut self, now: Duration) -> Result<AdmissionClaims, AuthorityError>;

    /// Issue the next opaque one-use token for an admitted active session.
    fn issue_resume(
        &mut self,
        claims: &AdmissionClaims,
        now: Duration,
    ) -> Result<IssuedResumeToken, AuthorityError>;

    /// Atomically consume a resume token and assign a fresh connection epoch.
    fn consume_resume(
        &mut self,
        token: &[u8],
        connection_epoch: ConnectionEpoch,
        now: Duration,
    ) -> Result<ResumeClaims, AuthorityError>;
}

/// Validate the protocol-level resume token bound before authority work.
pub fn validate_resume_token(token: &[u8]) -> Result<(), AuthorityError> {
    validate_opaque(token, MAX_RESUME_TOKEN_BYTES)
}

fn validate_opaque(value: &[u8], maximum: usize) -> Result<(), AuthorityError> {
    if value.is_empty() {
        return Err(AuthorityError::Invalid);
    }
    if value.len() > maximum {
        return Err(AuthorityError::Wire(WireError::Oversized {
            actual: value.len(),
            maximum,
        }));
    }
    Ok(())
}
