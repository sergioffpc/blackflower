use std::num::NonZeroU64;

use crate::codec::{Reader, Writer};
use crate::wire::{MAX_CONTROL_MESSAGE_BYTES, WireError, decode_frame, encode_frame};
use crate::{
    BootstrapId, CommandId, ConnectionEpoch, MAX_MAP_ID_BYTES, MapId, MatchId, PlayerId,
    ProjectionDigest, ProtocolRevision, RequiredContentSetId, SessionId, SimulationTick,
};

/// Maximum opaque one-use resume token size.
pub const MAX_RESUME_TOKEN_BYTES: usize = 512;

/// Session identities assigned by the server after protocol negotiation.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub struct AdmissionClaims {
    /// Game session identity.
    pub session_id: SessionId,
    /// Player identity.
    pub player_id: PlayerId,
    /// Match identity.
    pub match_id: MatchId,
    /// Exact application protocol revision.
    pub protocol_revision: ProtocolRevision,
}

/// Server-selected map and exact signed package-set identity required for it.
#[derive(Debug, Clone, PartialEq, Eq)]
pub struct ContentManifest {
    /// Portable logical map selected for the admitted session.
    pub map_id: MapId,
    /// Exact ordered package-set identity required by the server.
    pub required_content_set_id: RequiredContentSetId,
}

/// Server-authorized controlled object and its non-reusing generation.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub struct ControlBinding {
    /// Generation incremented whenever the controlled object changes.
    pub control_epoch: u32,
    /// Non-zero replicated identity owned by this session.
    pub controlled_entity: NonZeroU64,
}

/// Stable reason why the client cannot enter the server-selected map.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
#[repr(u8)]
pub enum ContentRejectReason {
    /// The locally installed signed package set differs from the requirement.
    AssetSetMismatch = 1,
}

impl TryFrom<u8> for ContentRejectReason {
    type Error = WireError;

    fn try_from(value: u8) -> Result<Self, Self::Error> {
        match value {
            1 => Ok(Self::AssetSetMismatch),
            _ => Err(WireError::InvalidValue("content rejection reason")),
        }
    }
}

/// Stable admission rejection sent before an authoritative player is created.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
#[repr(u8)]
pub enum AdmissionRejectReason {
    /// The application protocol revision differs.
    Incompatible = 1,
    /// Bootstrap capacity is not currently available.
    ServerBusy = 2,
    /// The peer violated the session protocol.
    ProtocolViolation = 3,
    /// The server could not assign session identities.
    IdentityUnavailable = 4,
}

impl TryFrom<u8> for AdmissionRejectReason {
    type Error = WireError;

    fn try_from(value: u8) -> Result<Self, Self::Error> {
        match value {
            1 => Ok(Self::Incompatible),
            2 => Ok(Self::ServerBusy),
            3 => Ok(Self::ProtocolViolation),
            4 => Ok(Self::IdentityUnavailable),
            _ => Err(WireError::InvalidValue("admission rejection reason")),
        }
    }
}

/// Reason a full state resynchronization is required.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
#[repr(u8)]
pub enum ResyncReason {
    /// An acknowledged baseline is no longer retained.
    BaselineUnavailable = 1,
    /// Applied snapshots stopped progressing.
    SnapshotStalled = 2,
    /// Prediction or input history cannot cover reconciliation.
    PredictionHistoryMissing = 3,
    /// Clock uncertainty makes time-dependent state unsafe.
    ClockUnsafe = 4,
    /// Essential replicated state does not fit the chunk budget.
    EssentialSnapshotTooLarge = 5,
}

impl TryFrom<u8> for ResyncReason {
    type Error = WireError;

    fn try_from(value: u8) -> Result<Self, Self::Error> {
        match value {
            1 => Ok(Self::BaselineUnavailable),
            2 => Ok(Self::SnapshotStalled),
            3 => Ok(Self::PredictionHistoryMissing),
            4 => Ok(Self::ClockUnsafe),
            5 => Ok(Self::EssentialSnapshotTooLarge),
            _ => Err(WireError::InvalidValue("resync reason")),
        }
    }
}

/// Stable disposition of one idempotent discrete command.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum CommandDisposition {
    /// Command passed ingress validation and awaits execution.
    Queued {
        /// Authoritative tick selected for execution.
        effective_tick: SimulationTick,
    },
    /// Command effects were committed authoritatively.
    Committed {
        /// Tick at which effects were committed.
        effective_tick: SimulationTick,
    },
    /// Command was rejected without mutating state.
    Rejected {
        /// Stable semantic rejection code owned by the command registry.
        reason: u16,
    },
    /// A newer command replaced this command.
    Superseded {
        /// Identity of the replacing command.
        replacing_command: CommandId,
    },
}

/// Reliable session-control message.
#[derive(Debug, Clone, PartialEq, Eq)]
pub enum SessionControlMessage {
    /// Negotiate the exact application protocol revision before session creation.
    AdmissionRequest {
        /// Client application protocol revision.
        protocol_revision: ProtocolRevision,
    },
    /// Confirm session identities and assign the initial connection generation.
    AdmissionAccepted {
        /// Server-assigned session identities and accepted protocol revision.
        claims: AdmissionClaims,
        /// Server-assigned generation used by every datagram on this connection.
        connection_epoch: ConnectionEpoch,
    },
    /// Reject admission before creating authoritative player state.
    AdmissionRejected(AdmissionRejectReason),
    /// Declare the server-selected map and its exact signed content requirement.
    ContentManifest(ContentManifest),
    /// Confirm that the declared map content is installed and verified locally.
    ContentReady(ContentManifest),
    /// Reject the declared map before any bootstrap state is applied.
    ContentRejected(ContentRejectReason),
    /// Announce the next full snapshot stream.
    BootstrapOffer {
        /// Transfer identity.
        bootstrap_id: BootstrapId,
        /// Tick represented by the full state.
        snapshot_tick: SimulationTick,
        /// Expected projection digest after application.
        digest: ProjectionDigest,
        /// Exact uncompressed byte length.
        length: u32,
    },
    /// Confirm that a full snapshot was reconstructed and applied.
    BootstrapApplied {
        /// Transfer identity.
        bootstrap_id: BootstrapId,
        /// Applied snapshot tick.
        snapshot_tick: SimulationTick,
        /// Digest of the applied projection.
        digest: ProjectionDigest,
    },
    /// Schedule the newly synchronized client for activation.
    ActivateAt { tick: SimulationTick },
    /// Assign the object whose controls this session may submit.
    ControlBinding(ControlBinding),
    /// Ask for a bounded full-state resynchronization.
    ResyncRequest { reason: ResyncReason },
    /// Present a one-use reconnect token on a fresh connection.
    ResumeRequest { token: Vec<u8> },
    /// Deliver the replacement one-use reconnect token.
    ResumeIssued {
        /// Opaque token bytes.
        token: Vec<u8>,
        /// Lifetime from issuance, in milliseconds.
        expires_in_millis: u32,
    },
    /// Confirm that the admission clock burst is safe for first activation.
    ClockSynchronized {
        /// Client-observed uncertainty rounded upward to simulation ticks.
        uncertainty_ticks: u16,
    },
    /// Report final or intermediate command disposition.
    CommandDisposition {
        /// Command being reported.
        command_id: CommandId,
        /// Current authoritative disposition.
        disposition: CommandDisposition,
    },
    /// Gracefully close the application session.
    Closing { code: u16 },
}

const ADMISSION_REQUEST: u8 = 1;
const ADMISSION_ACCEPTED: u8 = 2;
const ADMISSION_REJECTED: u8 = 3;
const BOOTSTRAP_OFFER: u8 = 4;
const BOOTSTRAP_APPLIED: u8 = 5;
const ACTIVATE_AT: u8 = 6;
const RESYNC_REQUEST: u8 = 7;
const RESUME_REQUEST: u8 = 8;
const RESUME_ISSUED: u8 = 9;
const COMMAND_DISPOSITION: u8 = 10;
const CLOSING: u8 = 11;
const CLOCK_SYNCHRONIZED: u8 = 12;
const CONTENT_MANIFEST: u8 = 13;
const CONTENT_READY: u8 = 14;
const CONTENT_REJECTED: u8 = 15;
const CONTROL_BINDING: u8 = 16;

/// Encode one framed reliable session-control message.
pub fn encode_control_message(message: &SessionControlMessage) -> Result<Vec<u8>, WireError> {
    let (kind, payload) = encode_control_payload(message)?;
    encode_frame(kind, &payload, MAX_CONTROL_MESSAGE_BYTES)
}

/// Decode one exact framed reliable session-control message.
pub fn decode_control_message(bytes: &[u8]) -> Result<SessionControlMessage, WireError> {
    let (kind, payload) = decode_frame(bytes, MAX_CONTROL_MESSAGE_BYTES)?;
    let mut reader = Reader::new(payload);
    let message = decode_control_payload(kind, &mut reader)?;
    reader.finish()?;
    Ok(message)
}

#[allow(
    clippy::too_many_lines,
    reason = "one exhaustive match keeps every reliable control kind visibly encoded"
)]
fn encode_control_payload(message: &SessionControlMessage) -> Result<(u8, Vec<u8>), WireError> {
    match message {
        SessionControlMessage::AdmissionRequest { protocol_revision } => Ok((
            ADMISSION_REQUEST,
            protocol_revision.get().to_le_bytes().to_vec(),
        )),
        SessionControlMessage::AdmissionAccepted {
            claims,
            connection_epoch,
        } => encode_admission_accepted_control(claims, *connection_epoch),
        SessionControlMessage::AdmissionRejected(reason) => Ok(encode_admission_rejected(*reason)),
        SessionControlMessage::ContentManifest(manifest) => {
            encode_content_control(ContentControl::Manifest(manifest))
        }
        SessionControlMessage::ContentReady(manifest) => {
            encode_content_control(ContentControl::Ready(manifest))
        }
        SessionControlMessage::ContentRejected(reason) => {
            encode_content_control(ContentControl::Rejected(*reason))
        }
        SessionControlMessage::BootstrapOffer {
            bootstrap_id,
            snapshot_tick,
            digest,
            length,
        } => Ok(encode_bootstrap_offer_control(
            *bootstrap_id,
            *snapshot_tick,
            digest,
            *length,
        )),
        SessionControlMessage::BootstrapApplied {
            bootstrap_id,
            snapshot_tick,
            digest,
        } => Ok((
            BOOTSTRAP_APPLIED,
            encode_bootstrap_applied(*bootstrap_id, *snapshot_tick, digest),
        )),
        SessionControlMessage::ActivateAt { tick } => Ok(encode_activate_at(*tick)),
        SessionControlMessage::ControlBinding(binding) => {
            Ok((CONTROL_BINDING, encode_control_binding(*binding)))
        }
        SessionControlMessage::ResyncRequest { reason } => Ok(encode_resync_request(*reason)),
        SessionControlMessage::ResumeRequest { token } => {
            encode_resume_token(RESUME_REQUEST, token, None)
        }
        SessionControlMessage::ResumeIssued {
            token,
            expires_in_millis,
        } => encode_resume_token(RESUME_ISSUED, token, Some(*expires_in_millis)),
        SessionControlMessage::ClockSynchronized { uncertainty_ticks } => {
            Ok((CLOCK_SYNCHRONIZED, uncertainty_ticks.to_le_bytes().to_vec()))
        }
        SessionControlMessage::CommandDisposition {
            command_id,
            disposition,
        } => Ok((
            COMMAND_DISPOSITION,
            encode_disposition(*command_id, *disposition),
        )),
        SessionControlMessage::Closing { code } => Ok((CLOSING, code.to_le_bytes().to_vec())),
    }
}

fn encode_admission_rejected(reason: AdmissionRejectReason) -> (u8, Vec<u8>) {
    (ADMISSION_REJECTED, vec![reason as u8])
}

enum ContentControl<'a> {
    Manifest(&'a ContentManifest),
    Ready(&'a ContentManifest),
    Rejected(ContentRejectReason),
}

fn encode_content_control(message: ContentControl<'_>) -> Result<(u8, Vec<u8>), WireError> {
    match message {
        ContentControl::Manifest(manifest) => {
            Ok((CONTENT_MANIFEST, encode_content_manifest(manifest)?))
        }
        ContentControl::Ready(manifest) => Ok((CONTENT_READY, encode_content_manifest(manifest)?)),
        ContentControl::Rejected(reason) => Ok((CONTENT_REJECTED, vec![reason as u8])),
    }
}

fn encode_activate_at(tick: SimulationTick) -> (u8, Vec<u8>) {
    (ACTIVATE_AT, tick.get().to_le_bytes().to_vec())
}

fn encode_resync_request(reason: ResyncReason) -> (u8, Vec<u8>) {
    (RESYNC_REQUEST, vec![reason as u8])
}

fn encode_admission_accepted_control(
    claims: &AdmissionClaims,
    connection_epoch: ConnectionEpoch,
) -> Result<(u8, Vec<u8>), WireError> {
    if connection_epoch.get() == 0 {
        return Err(WireError::InvalidValue("connection epoch"));
    }
    Ok((
        ADMISSION_ACCEPTED,
        encode_admission_accepted(claims, connection_epoch),
    ))
}

fn encode_bootstrap_offer_control(
    bootstrap_id: BootstrapId,
    snapshot_tick: SimulationTick,
    digest: &ProjectionDigest,
    length: u32,
) -> (u8, Vec<u8>) {
    (
        BOOTSTRAP_OFFER,
        encode_bootstrap_offer(bootstrap_id, snapshot_tick, digest, length),
    )
}

fn decode_control_payload(
    kind: u8,
    reader: &mut Reader<'_>,
) -> Result<SessionControlMessage, WireError> {
    match kind {
        ADMISSION_REQUEST => Ok(SessionControlMessage::AdmissionRequest {
            protocol_revision: ProtocolRevision::new(reader.u32()?),
        }),
        ADMISSION_ACCEPTED => decode_admission_accepted(reader),
        ADMISSION_REJECTED => Ok(SessionControlMessage::AdmissionRejected(
            AdmissionRejectReason::try_from(reader.u8()?)?,
        )),
        CONTENT_MANIFEST => Ok(SessionControlMessage::ContentManifest(
            decode_content_manifest(reader)?,
        )),
        CONTENT_READY => Ok(SessionControlMessage::ContentReady(
            decode_content_manifest(reader)?,
        )),
        CONTENT_REJECTED => Ok(SessionControlMessage::ContentRejected(
            ContentRejectReason::try_from(reader.u8()?)?,
        )),
        BOOTSTRAP_OFFER => decode_bootstrap_offer(reader),
        BOOTSTRAP_APPLIED => decode_bootstrap_applied(reader),
        ACTIVATE_AT => Ok(SessionControlMessage::ActivateAt {
            tick: SimulationTick::new(reader.u64()?),
        }),
        CONTROL_BINDING => Ok(SessionControlMessage::ControlBinding(
            decode_control_binding(reader)?,
        )),
        RESYNC_REQUEST => Ok(SessionControlMessage::ResyncRequest {
            reason: ResyncReason::try_from(reader.u8()?)?,
        }),
        RESUME_REQUEST => decode_resume_request(reader),
        RESUME_ISSUED => decode_resume_issued(reader),
        CLOCK_SYNCHRONIZED => Ok(SessionControlMessage::ClockSynchronized {
            uncertainty_ticks: reader.u16()?,
        }),
        COMMAND_DISPOSITION => decode_disposition(reader),
        CLOSING => Ok(SessionControlMessage::Closing {
            code: reader.u16()?,
        }),
        value => Err(WireError::UnknownMessage(value)),
    }
}

fn encode_control_binding(binding: ControlBinding) -> Vec<u8> {
    let mut writer = Writer::with_capacity(12);
    writer.u32(binding.control_epoch);
    writer.u64(binding.controlled_entity.get());
    writer.finish()
}

fn decode_control_binding(reader: &mut Reader<'_>) -> Result<ControlBinding, WireError> {
    let control_epoch = reader.u32()?;
    let controlled_entity = NonZeroU64::new(reader.u64()?)
        .ok_or(WireError::InvalidValue("controlled entity is zero"))?;
    Ok(ControlBinding {
        control_epoch,
        controlled_entity,
    })
}

fn encode_resume_token(
    kind: u8,
    token: &[u8],
    expires_in_millis: Option<u32>,
) -> Result<(u8, Vec<u8>), WireError> {
    if token.len() > MAX_RESUME_TOKEN_BYTES {
        return Err(WireError::Oversized {
            actual: token.len(),
            maximum: MAX_RESUME_TOKEN_BYTES,
        });
    }
    let mut writer = Writer::with_capacity(6 + token.len());
    writer.bytes_u16(token)?;
    if let Some(expires) = expires_in_millis {
        writer.u32(expires);
    }
    Ok((kind, writer.finish()))
}

fn encode_claims(claims: &AdmissionClaims) -> Vec<u8> {
    let mut writer = Writer::with_capacity(52);
    writer.fixed(claims.session_id.as_bytes());
    writer.fixed(claims.player_id.as_bytes());
    writer.fixed(claims.match_id.as_bytes());
    writer.u32(claims.protocol_revision.get());
    writer.finish()
}

fn encode_admission_accepted(
    claims: &AdmissionClaims,
    connection_epoch: ConnectionEpoch,
) -> Vec<u8> {
    let mut payload = encode_claims(claims);
    payload.extend_from_slice(&connection_epoch.get().to_le_bytes());
    payload
}

fn decode_admission_accepted(reader: &mut Reader<'_>) -> Result<SessionControlMessage, WireError> {
    let claims = decode_claims(reader)?;
    let connection_epoch = ConnectionEpoch::new(reader.u32()?);
    if connection_epoch.get() == 0 {
        return Err(WireError::InvalidValue("connection epoch"));
    }
    Ok(SessionControlMessage::AdmissionAccepted {
        claims,
        connection_epoch,
    })
}

fn decode_claims(reader: &mut Reader<'_>) -> Result<AdmissionClaims, WireError> {
    Ok(AdmissionClaims {
        session_id: SessionId::from_bytes(reader.fixed()?),
        player_id: PlayerId::from_bytes(reader.fixed()?),
        match_id: MatchId::from_bytes(reader.fixed()?),
        protocol_revision: ProtocolRevision::new(reader.u32()?),
    })
}

fn encode_content_manifest(manifest: &ContentManifest) -> Result<Vec<u8>, WireError> {
    let mut writer = Writer::with_capacity(2 + manifest.map_id.as_str().len() + 32);
    writer.bytes_u16(manifest.map_id.as_str().as_bytes())?;
    writer.fixed(manifest.required_content_set_id.as_bytes());
    Ok(writer.finish())
}

fn decode_content_manifest(reader: &mut Reader<'_>) -> Result<ContentManifest, WireError> {
    let map_bytes = reader.bytes_u16(MAX_MAP_ID_BYTES)?;
    let map_text =
        std::str::from_utf8(&map_bytes).map_err(|_error| WireError::InvalidValue("map id"))?;
    let map_id = map_text
        .parse()
        .map_err(|_error| WireError::InvalidValue("map id"))?;
    Ok(ContentManifest {
        map_id,
        required_content_set_id: RequiredContentSetId::from_bytes(reader.fixed()?),
    })
}

fn encode_bootstrap_offer(
    bootstrap_id: BootstrapId,
    tick: SimulationTick,
    digest: &ProjectionDigest,
    length: u32,
) -> Vec<u8> {
    let mut writer = Writer::with_capacity(52);
    writer.u64(bootstrap_id.get());
    writer.u64(tick.get());
    writer.fixed(digest.as_bytes());
    writer.u32(length);
    writer.finish()
}

fn encode_bootstrap_applied(
    bootstrap_id: BootstrapId,
    tick: SimulationTick,
    digest: &ProjectionDigest,
) -> Vec<u8> {
    let mut writer = Writer::with_capacity(48);
    writer.u64(bootstrap_id.get());
    writer.u64(tick.get());
    writer.fixed(digest.as_bytes());
    writer.finish()
}

fn decode_bootstrap_offer(reader: &mut Reader<'_>) -> Result<SessionControlMessage, WireError> {
    Ok(SessionControlMessage::BootstrapOffer {
        bootstrap_id: BootstrapId::new(reader.u64()?),
        snapshot_tick: SimulationTick::new(reader.u64()?),
        digest: ProjectionDigest::from_bytes(reader.fixed()?),
        length: reader.u32()?,
    })
}

fn decode_bootstrap_applied(reader: &mut Reader<'_>) -> Result<SessionControlMessage, WireError> {
    Ok(SessionControlMessage::BootstrapApplied {
        bootstrap_id: BootstrapId::new(reader.u64()?),
        snapshot_tick: SimulationTick::new(reader.u64()?),
        digest: ProjectionDigest::from_bytes(reader.fixed()?),
    })
}

fn decode_resume_request(reader: &mut Reader<'_>) -> Result<SessionControlMessage, WireError> {
    Ok(SessionControlMessage::ResumeRequest {
        token: reader.bytes_u16(MAX_RESUME_TOKEN_BYTES)?,
    })
}

fn decode_resume_issued(reader: &mut Reader<'_>) -> Result<SessionControlMessage, WireError> {
    Ok(SessionControlMessage::ResumeIssued {
        token: reader.bytes_u16(MAX_RESUME_TOKEN_BYTES)?,
        expires_in_millis: reader.u32()?,
    })
}

fn encode_disposition(command_id: CommandId, disposition: CommandDisposition) -> Vec<u8> {
    let mut writer = Writer::with_capacity(17);
    writer.u64(command_id.get());
    match disposition {
        CommandDisposition::Queued { effective_tick } => {
            write_tick_disposition(&mut writer, 1, effective_tick)
        }
        CommandDisposition::Committed { effective_tick } => {
            write_tick_disposition(&mut writer, 2, effective_tick)
        }
        CommandDisposition::Rejected { reason } => {
            writer.u8(3);
            writer.u16(reason);
        }
        CommandDisposition::Superseded { replacing_command } => {
            writer.u8(4);
            writer.u64(replacing_command.get());
        }
    }
    writer.finish()
}

fn write_tick_disposition(writer: &mut Writer, kind: u8, tick: SimulationTick) {
    writer.u8(kind);
    writer.u64(tick.get());
}

fn decode_disposition(reader: &mut Reader<'_>) -> Result<SessionControlMessage, WireError> {
    let command_id = CommandId::new(reader.u64()?);
    let disposition = match reader.u8()? {
        1 => CommandDisposition::Queued {
            effective_tick: SimulationTick::new(reader.u64()?),
        },
        2 => CommandDisposition::Committed {
            effective_tick: SimulationTick::new(reader.u64()?),
        },
        3 => CommandDisposition::Rejected {
            reason: reader.u16()?,
        },
        4 => CommandDisposition::Superseded {
            replacing_command: CommandId::new(reader.u64()?),
        },
        _ => return Err(WireError::InvalidValue("command disposition")),
    };
    Ok(SessionControlMessage::CommandDisposition {
        command_id,
        disposition,
    })
}
