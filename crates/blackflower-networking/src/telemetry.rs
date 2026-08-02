use std::time::Duration;

/// Stable traffic direction label; never contains a session or player ID.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum MetricDirection {
    /// Client-to-server traffic.
    Upstream,
    /// Server-to-client traffic.
    Downstream,
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
        "Estimated clock uncertainty rounded up to simulation ticks"
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
        "Snapshot generations sent or applied"
    );
    metrics::describe_counter!(
        "blackflower_network_inputs_total",
        Unit::Count,
        "Canonical input frames accepted"
    );
    metrics::describe_histogram!(
        "blackflower_network_bootstrap_bytes",
        Unit::Bytes,
        "Uncompressed full-state bootstrap size"
    );
    metrics::describe_counter!(
        "blackflower_network_resync_total",
        Unit::Count,
        "Full-state post-activation resynchronizations"
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

/// Publish one bounded queue depth.
pub fn record_queue_depth(kind: QueueKind, depth: usize) {
    metrics::gauge!("blackflower_network_queue_depth", "queue" => kind.as_str())
        .set(metric_u32_usize(depth));
}

/// Add UDP bytes reported by Quinn after application estimate reconciliation.
pub fn record_udp_bytes(direction: MetricDirection, bytes: usize) {
    metrics::counter!(
        "blackflower_network_udp_bytes_total",
        "direction" => direction.as_str()
    )
    .increment(u64::try_from(bytes).unwrap_or(u64::MAX));
}

/// Record one intentional or capacity-induced application drop.
pub fn record_drop(reason: DropReason) {
    metrics::counter!("blackflower_network_drops_total", "reason" => reason.as_str()).increment(1);
}

/// Record one emitted or applied snapshot generation.
pub fn record_snapshot(action: &'static str) {
    metrics::counter!("blackflower_network_snapshots_total", "action" => action).increment(1);
}

/// Record accepted canonical input frames.
pub fn record_inputs(count: usize) {
    metrics::counter!("blackflower_network_inputs_total")
        .increment(u64::try_from(count).unwrap_or(u64::MAX));
}

/// Record one full-state bootstrap size.
pub fn record_bootstrap(bytes: usize) {
    metrics::histogram!("blackflower_network_bootstrap_bytes").record(metric_u32_usize(bytes));
}

/// Record one post-activation resynchronization.
pub fn record_resync() {
    metrics::counter!("blackflower_network_resync_total").increment(1);
}

/// Record voice traffic in a bounded direction.
pub fn record_voice(direction: MetricDirection) {
    metrics::counter!(
        "blackflower_network_voice_packets_total",
        "direction" => direction.as_str()
    )
    .increment(1);
}

/// Record a bounded protocol-violation class.
pub fn record_protocol_violation(kind: ViolationKind) {
    metrics::counter!(
        "blackflower_network_protocol_violations_total",
        "kind" => kind.as_str()
    )
    .increment(1);
}

impl MetricDirection {
    const fn as_str(self) -> &'static str {
        match self {
            Self::Upstream => "upstream",
            Self::Downstream => "downstream",
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
