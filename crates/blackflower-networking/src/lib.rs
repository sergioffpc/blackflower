#![doc = include_str!("../README.md")]

mod authority;
mod clock;
mod codec;
mod control;
mod datagram_link;
mod datagrams;
mod input;
mod scheduler;
mod session;
mod telemetry;
mod types;
mod voice;
mod wire;

pub use authority::{
    ADMISSION_TICKET_LIFETIME, AuthorityError, IssuedResumeToken, RECONNECT_WINDOW, ResumeClaims,
    SessionAuthority, validate_admission_ticket, validate_resume_token,
};
pub use clock::{
    ACTIVE_TIME_SYNC_INTERVAL, CLOCK_SAMPLE_TIMEOUT, ClockError, ClockFilter, ClockSafety,
    ClockSample, INITIAL_INPUT_LEAD_TICKS, INITIAL_TIME_SYNC_INTERVAL, INITIAL_TIME_SYNC_SAMPLES,
    NETWORK_TICK_RATE_HZ, TimeSyncSchedule, input_lead_ticks, server_micros_to_tick,
};
pub use control::{
    AdmissionClaims, AdmissionRejectReason, CommandDisposition, MAX_ADMISSION_TICKET_BYTES,
    MAX_RESUME_TOKEN_BYTES, ResyncReason, SessionControlMessage, decode_control_message,
    encode_control_message,
};
pub use datagram_link::{DatagramLinkEndpoint, DatagramLinkError, InMemoryDatagramLink};
pub use datagrams::{
    CommandTimingClass, ControlFrame, DiscreteCommand, InputDatagram, MAX_COMMAND_BYTES,
    MAX_COMMANDS, MAX_CONTROL_FRAME_BYTES, MAX_CONTROL_FRAMES, MAX_INPUT_DATAGRAM_PAYLOAD_BYTES,
    MAX_SNAPSHOT_CHUNKS, STATE_BOOTSTRAP_HEADER_BYTES, SnapshotAppliedAck, SnapshotChunk,
    StateBootstrapHeader, TimeSyncMessage, decode_input_datagram, decode_snapshot_applied_ack,
    decode_snapshot_chunk, decode_state_bootstrap_header, decode_time_sync, encode_input_datagram,
    encode_snapshot_applied_ack, encode_snapshot_chunk, encode_state_bootstrap_header,
    encode_time_sync,
};
pub use input::{
    CodecViolation, CommandCodec, CommandRejection, CommandTimingDecision, ControlCodec,
    Deduplication, DeduplicationError, HistoricalCommandContext, INPUT_FAILSAFE_TICKS,
    INPUT_GRACE_TICKS, INPUT_HISTORY_TICKS, InputDeduplicator, InputHealth,
    MAX_CATCH_UP_BALLISTIC_TICKS, MAX_FUTURE_COMMAND_TICKS, MAX_REWIND_RAY_TICKS,
    MAX_ROLLBACK_TICKS, classify_command, input_health, validate_command_codec,
    validate_control_codec,
};
pub use scheduler::{
    BandwidthScheduler, BudgetTier, CONSTRAINED_DOWNSTREAM_BITS_PER_SECOND,
    CONSTRAINED_MATCH_BITS_PER_SECOND, CONSTRAINED_UPSTREAM_BITS_PER_SECOND,
    MAX_CONTROL_QUEUE_BYTES, MAX_HOST_EVENTS, MAX_PENDING_SNAPSHOTS, MAXIMUM_SNAPSHOT_RATE_HZ,
    MINIMUM_SNAPSHOT_RATE_HZ, MatchEgressBudget, NetworkQueues,
    PREFERRED_DOWNSTREAM_BITS_PER_SECOND, PREFERRED_MATCH_BITS_PER_SECOND,
    PREFERRED_UPSTREAM_BITS_PER_SECOND, QueueError, ScheduledPayload, TrafficClass,
    TrafficDirection,
};
pub use session::{
    ACTIVATION_ALIGNMENT_TICKS, ClientSession, CompatibilityContract, MAX_RESYNCS_PER_WINDOW,
    MINIMUM_ACTIVATION_LEAD_TICKS, OperationalState, RESYNC_WINDOW, ServerSession, SessionError,
    SessionState, activation_tick, operational_state,
};
pub use telemetry::{
    DropReason, MetricDirection, QueueKind, ViolationKind, connection_closed, connection_opened,
    describe_network_metrics, record_bootstrap, record_clock_uncertainty, record_drop,
    record_inputs, record_protocol_violation, record_queue_depth, record_resync, record_rtt,
    record_snapshot, record_udp_bytes, record_voice,
};
pub use types::{
    BootstrapId, CommandId, ConnectionEpoch, FlowSequence, InputSequence, MatchId, PlayerId,
    ProjectionDigest, ProtocolRevision, RequiredContentSetId, SessionId, SimulationCompatibilityId,
    SimulationTick, VoiceStreamId,
};
pub use voice::{
    MAX_AUDIBLE_VOICES, MAX_QUEUED_VOICE_PACKETS, VOICE_FRAME_MILLIS, VOICE_JITTER_MILLIS,
    VOICE_MAXIMUM_KBPS, VOICE_TARGET_KBPS, VoiceBindings, VoiceChannel, VoiceError,
    validate_voice_deliveries,
};
pub use wire::{
    DATAGRAM_HEADER_BYTES, DatagramHeader, DecodedDatagram, FlowId, MAX_BOOTSTRAP_BYTES,
    MAX_CONTROL_MESSAGE_BYTES, MINIMUM_QUIC_DATAGRAM_BYTES, MINIMUM_USEFUL_DATAGRAM_BYTES,
    StreamKind, TARGET_BOOTSTRAP_BYTES, WIRE_VERSION, WireError, decode_datagram, decode_frame,
    decode_stream_preamble, encode_datagram, encode_frame, encode_stream_preamble,
    projection_digest,
};
