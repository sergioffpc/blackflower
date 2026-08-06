use std::time::Duration;

/// Stable traffic direction label; never contains a session or player ID.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum MetricDirection {
    /// Client-to-server traffic.
    Upstream,
    /// Server-to-client traffic.
    Downstream,
}

/// Stable client/server input lifecycle action.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum InputAction {
    /// A client committed an input frame to the latest-wins transport queue.
    Submitted,
    /// The authoritative server accepted a canonical input frame.
    Accepted,
}

/// Stable snapshot lifecycle action.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum SnapshotAction {
    /// The authoritative server queued a snapshot generation.
    Sent,
    /// A client applied a complete bootstrap or incremental snapshot.
    Applied,
    /// The authoritative server accepted an applied-snapshot acknowledgement.
    Acknowledged,
}

/// Stable resynchronization lifecycle action.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum ResyncAction {
    /// A client requested a full-state resynchronization.
    Requested,
    /// The authoritative server accepted and started a resynchronization.
    Started,
}

/// Stable bounded clock-synchronization state.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum ClockState {
    /// Sessions that have reported an activation-safe clock estimate.
    Synchronized,
    /// Sessions still waiting for an activation-safe clock estimate.
    Unsynchronized,
}

/// Stable bounded queue label.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum QueueKind {
    /// Reliable session control.
    Control,
    /// Full-state bootstrap.
    Bootstrap,
    /// Latest canonical input.
    Input,
    /// Pending snapshot generations.
    Snapshot,
    /// Live voice packets.
    Voice,
    /// Events awaiting the synchronous host.
    Host,
}

/// Stable application drop reason.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum DropReason {
    /// A newer latest-wins payload superseded this one.
    Superseded,
    /// Deadline elapsed before transport admission.
    Deadline,
    /// Shaping budget was unavailable.
    Budget,
    /// Queue capacity was exhausted.
    QueueFull,
    /// Datagram arrived outside its reorder window.
    Late,
}

/// Stable protocol-violation class without peer identity labels.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum ViolationKind {
    /// Invalid framing, bounds, version, or reserved bits.
    Wire,
    /// Invalid lifecycle transition or stream role.
    Session,
    /// Conflicting reuse of an input or command identity.
    ConflictingIdentity,
    /// Incompatible admission identities.
    Compatibility,
    /// Voice stream or routing violation.
    Voice,
}

/// Register descriptions and units for all network metrics.
#[allow(
    clippy::too_many_lines,
    reason = "metric declarations form one auditable registry"
)]
pub fn describe_network_metrics() {
    use metrics::Unit;

    metrics::describe_gauge!(
        "blackflower_network_connections",
        Unit::Count,
        "Current QUIC application connections"
    );
    metrics::describe_histogram!(
        "blackflower_network_rtt_seconds",
        Unit::Seconds,
        "QUIC smoothed round-trip time"
    );
    metrics::describe_gauge!(
        "blackflower_network_clock_uncertainty_ticks",
        Unit::Count,
        "Maximum estimated clock uncertainty rounded up to simulation ticks"
    );
    metrics::describe_gauge!(
        "blackflower_network_clock_sessions",
        Unit::Count,
        "Current network sessions by bounded clock synchronization state"
    );
    metrics::describe_gauge!(
        "blackflower_network_queue_depth",
        Unit::Count,
        "Bounded network queue depth"
    );
    metrics::describe_counter!(
        "blackflower_network_udp_bytes_total",
        Unit::Bytes,
        "UDP bytes reported by Quinn"
    );
    metrics::describe_counter!(
        "blackflower_network_drops_total",
        Unit::Count,
        "Application payloads dropped before or after transport"
    );
    metrics::describe_counter!(
        "blackflower_network_snapshots_total",
        Unit::Count,
        "Snapshot lifecycle transitions by bounded action"
    );
    metrics::describe_counter!(
        "blackflower_network_inputs_total",
        Unit::Count,
        "Canonical input frames by bounded client/server action"
    );
    metrics::describe_histogram!(
        "blackflower_network_bootstrap_bytes",
        Unit::Bytes,
        "Uncompressed full-state bootstrap size"
    );
    metrics::describe_counter!(
        "blackflower_network_resync_total",
        Unit::Count,
        "Full-state post-activation resynchronizations by bounded action"
    );
    metrics::describe_counter!(
        "blackflower_network_voice_packets_total",
        Unit::Count,
        "Voice capture or delivery packets"
    );
    metrics::describe_counter!(
        "blackflower_network_protocol_violations_total",
        Unit::Count,
        "Protocol violations by bounded class"
    );
}

/// Register zero-valued bounded counter series before the first network event.
///
/// This keeps process dashboards explicit about inactive traffic instead of
/// making an inactive counter indistinguishable from missing instrumentation.
pub fn initialize_network_metrics() {
    describe_network_metrics();
    for action in [InputAction::Submitted, InputAction::Accepted] {
        record_inputs(action, 0);
    }
    for action in [ResyncAction::Requested, ResyncAction::Started] {
        record_resync_by(action, 0);
    }
    for kind in [
        QueueKind::Control,
        QueueKind::Bootstrap,
        QueueKind::Input,
        QueueKind::Snapshot,
        QueueKind::Voice,
        QueueKind::Host,
    ] {
        record_queue_depth_delta(kind, 0, 0);
    }
    for direction in [MetricDirection::Upstream, MetricDirection::Downstream] {
        record_udp_bytes(direction, 0);
        record_voice_by(direction, 0);
    }
    for reason in [
        DropReason::Superseded,
        DropReason::Deadline,
        DropReason::Budget,
        DropReason::QueueFull,
        DropReason::Late,
    ] {
        record_drop_by(reason, 0);
    }
    for action in [
        SnapshotAction::Sent,
        SnapshotAction::Applied,
        SnapshotAction::Acknowledged,
    ] {
        record_snapshot_by(action, 0);
    }
    for kind in [
        ViolationKind::Wire,
        ViolationKind::Session,
        ViolationKind::ConflictingIdentity,
        ViolationKind::Compatibility,
        ViolationKind::Voice,
    ] {
        record_protocol_violation_by(kind, 0);
    }
}

/// Increment the process connection gauge.
pub fn connection_opened() {
    metrics::gauge!("blackflower_network_connections").increment(1.0);
}

/// Decrement the process connection gauge.
pub fn connection_closed() {
    metrics::gauge!("blackflower_network_connections").decrement(1.0);
}

/// Record current QUIC smoothed RTT.
pub fn record_rtt(rtt: Duration) {
    metrics::histogram!("blackflower_network_rtt_seconds").record(rtt.as_secs_f64());
}

/// Publish clock uncertainty without peer identity labels.
pub fn record_clock_uncertainty(ticks: u64) {
    metrics::gauge!("blackflower_network_clock_uncertainty_ticks").set(metric_u32_u64(ticks));
}

/// Publish the current number of sessions in one bounded clock state.
pub fn record_clock_sessions(state: ClockState, count: usize) {
    metrics::gauge!("blackflower_network_clock_sessions", "state" => state.as_str())
        .set(metric_u32_usize(count));
}

/// Replace one connection's contribution to a process-wide queue depth.
pub fn record_queue_depth_delta(kind: QueueKind, previous: usize, current: usize) {
    let gauge = metrics::gauge!("blackflower_network_queue_depth", "queue" => kind.as_str());
    if current >= previous {
        gauge.increment(metric_u32_usize(current - previous));
    } else {
        gauge.decrement(metric_u32_usize(previous - current));
    }
}

/// Add one observed UDP-byte delta reported by Quinn.
pub fn record_udp_bytes(direction: MetricDirection, bytes: usize) {
    metrics::counter!(
        "blackflower_network_udp_bytes_total",
        "direction" => direction.as_str()
    )
    .increment(u64::try_from(bytes).unwrap_or(u64::MAX));
}

/// Record one intentional or capacity-induced application drop.
pub fn record_drop(reason: DropReason) {
    record_drop_by(reason, 1);
}

/// Record one snapshot lifecycle transition.
pub fn record_snapshot(action: SnapshotAction) {
    record_snapshot_by(action, 1);
}

/// Record canonical input frames at one client/server lifecycle boundary.
pub fn record_inputs(action: InputAction, count: usize) {
    metrics::counter!("blackflower_network_inputs_total", "action" => action.as_str())
        .increment(u64::try_from(count).unwrap_or(u64::MAX));
}

/// Record one full-state bootstrap size.
pub fn record_bootstrap(bytes: usize) {
    metrics::histogram!("blackflower_network_bootstrap_bytes").record(metric_u32_usize(bytes));
}

/// Record one post-activation resynchronization lifecycle transition.
pub fn record_resync(action: ResyncAction) {
    record_resync_by(action, 1);
}

/// Record voice traffic in a bounded direction.
pub fn record_voice(direction: MetricDirection) {
    record_voice_by(direction, 1);
}

fn record_voice_by(direction: MetricDirection, count: u64) {
    metrics::counter!(
        "blackflower_network_voice_packets_total",
        "direction" => direction.as_str()
    )
    .increment(count);
}

/// Record a bounded protocol-violation class.
pub fn record_protocol_violation(kind: ViolationKind) {
    record_protocol_violation_by(kind, 1);
}

fn record_drop_by(reason: DropReason, count: u64) {
    metrics::counter!("blackflower_network_drops_total", "reason" => reason.as_str())
        .increment(count);
}

fn record_snapshot_by(action: SnapshotAction, count: u64) {
    metrics::counter!("blackflower_network_snapshots_total", "action" => action.as_str())
        .increment(count);
}

fn record_resync_by(action: ResyncAction, count: u64) {
    metrics::counter!("blackflower_network_resync_total", "action" => action.as_str())
        .increment(count);
}

fn record_protocol_violation_by(kind: ViolationKind, count: u64) {
    metrics::counter!(
        "blackflower_network_protocol_violations_total",
        "kind" => kind.as_str()
    )
    .increment(count);
}

impl MetricDirection {
    const fn as_str(self) -> &'static str {
        match self {
            Self::Upstream => "upstream",
            Self::Downstream => "downstream",
        }
    }
}

impl InputAction {
    const fn as_str(self) -> &'static str {
        match self {
            Self::Submitted => "submitted",
            Self::Accepted => "accepted",
        }
    }
}

impl SnapshotAction {
    const fn as_str(self) -> &'static str {
        match self {
            Self::Sent => "sent",
            Self::Applied => "applied",
            Self::Acknowledged => "acknowledged",
        }
    }
}

impl ResyncAction {
    const fn as_str(self) -> &'static str {
        match self {
            Self::Requested => "requested",
            Self::Started => "started",
        }
    }
}

impl ClockState {
    const fn as_str(self) -> &'static str {
        match self {
            Self::Synchronized => "synchronized",
            Self::Unsynchronized => "unsynchronized",
        }
    }
}

impl QueueKind {
    const fn as_str(self) -> &'static str {
        match self {
            Self::Control => "control",
            Self::Bootstrap => "bootstrap",
            Self::Input => "input",
            Self::Snapshot => "snapshot",
            Self::Voice => "voice",
            Self::Host => "host",
        }
    }
}

impl DropReason {
    const fn as_str(self) -> &'static str {
        match self {
            Self::Superseded => "superseded",
            Self::Deadline => "deadline",
            Self::Budget => "budget",
            Self::QueueFull => "queue_full",
            Self::Late => "late",
        }
    }
}

impl ViolationKind {
    const fn as_str(self) -> &'static str {
        match self {
            Self::Wire => "wire",
            Self::Session => "session",
            Self::ConflictingIdentity => "conflicting_identity",
            Self::Compatibility => "compatibility",
            Self::Voice => "voice",
        }
    }
}

fn metric_u32_u64(value: u64) -> u32 {
    u32::try_from(value).unwrap_or(u32::MAX)
}

fn metric_u32_usize(value: usize) -> u32 {
    u32::try_from(value).unwrap_or(u32::MAX)
}
