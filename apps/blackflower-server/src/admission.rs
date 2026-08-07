use std::time::Duration;

use blackflower_networking::{
    AdmissionClaims, AuthorityError, CompatibilityContract, ConnectionEpoch, IssuedResumeToken,
    MatchId, PlayerId, RECONNECT_WINDOW, ResumeClaims, SessionAuthority, SessionId,
    validate_resume_token,
};

/// Credential-free one-process identity authority for the loopback vertical slice.
///
/// Authentication and matchmaking will replace this implementation. For now it
/// assigns unique process-local identities after exact protocol negotiation and
/// retains only the most recently issued reconnect credential.
#[derive(Debug)]
pub struct LoopbackSessionAuthority {
    contract: CompatibilityContract,
    next_admission: u64,
    resume_claims: Option<AdmissionClaims>,
    resume_token: Vec<u8>,
    resume_expires_at: Option<Duration>,
    resume_available: bool,
}

impl LoopbackSessionAuthority {
    /// Create a credential-free process-local identity authority.
    #[must_use]
    pub const fn new(contract: CompatibilityContract) -> Self {
        Self {
            contract,
            next_admission: 1,
            resume_claims: None,
            resume_token: Vec::new(),
            resume_expires_at: None,
            resume_available: false,
        }
    }
}

impl SessionAuthority for LoopbackSessionAuthority {
    fn admit(&mut self, _now: Duration) -> Result<AdmissionClaims, AuthorityError> {
        let identity = self.next_admission;
        self.next_admission = identity.checked_add(1).ok_or(AuthorityError::Unavailable)?;
        Ok(AdmissionClaims {
            session_id: SessionId::from_bytes(derive_128(
                "blackflower.loopback.session.v1",
                identity,
            )),
            player_id: PlayerId::from_bytes(derive_128("blackflower.loopback.player.v1", identity)),
            match_id: MatchId::from_bytes(derive_128("blackflower.loopback.match.v1", identity)),
            protocol_revision: self.contract.protocol_revision,
        })
    }

    fn issue_resume(
        &mut self,
        claims: &AdmissionClaims,
        now: Duration,
    ) -> Result<IssuedResumeToken, AuthorityError> {
        let expires_at = now.saturating_add(RECONNECT_WINDOW);
        self.resume_token = blake3::derive_key(
            "blackflower.loopback.resume.v1",
            claims.session_id.as_bytes(),
        )
        .to_vec();
        self.resume_claims = Some(*claims);
        self.resume_expires_at = Some(expires_at);
        self.resume_available = true;
        Ok(IssuedResumeToken {
            token: self.resume_token.clone(),
            expires_at,
        })
    }

    fn consume_resume(
        &mut self,
        token: &[u8],
        now: Duration,
    ) -> Result<ResumeClaims, AuthorityError> {
        validate_resume_token(token)?;
        if self.resume_expires_at.is_none_or(|expiry| now > expiry) {
            return Err(AuthorityError::Expired);
        }
        if token != self.resume_token {
            return Err(AuthorityError::Invalid);
        }
        if !std::mem::take(&mut self.resume_available) {
            return Err(AuthorityError::Replayed);
        }
        let claims = self.resume_claims.ok_or(AuthorityError::Invalid)?;
        Ok(ResumeClaims {
            session_id: claims.session_id,
            player_id: claims.player_id,
            match_id: claims.match_id,
            connection_epoch: ConnectionEpoch::new(u32::MAX),
        })
    }
}

fn derive_128(context: &'static str, identity: u64) -> [u8; 16] {
    let derived = blake3::derive_key(context, &identity.to_le_bytes());
    let mut bytes = [0_u8; 16];
    bytes.copy_from_slice(&derived[..16]);
    bytes
}
