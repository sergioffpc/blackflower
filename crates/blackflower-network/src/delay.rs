//! Artificial inbound latency for development and demonstration.
//!
//! A [`DelayQueue`] holds decoded messages until a per-message delivery
//! deadline elapses, then releases them. It models receive-side network
//! latency with jitter: each message is held for
//! `base ± rand(0..=jitter)`, clamped to non-negative. Because jitter is
//! applied per message, deadlines are *not* monotonic in arrival order —
//! a later arrival may become deliverable before an earlier one,
//! reordering packets exactly as an unreliable datagram channel would.
//! This is what exercises client-side reconciliation rather than merely
//! compensating for constant delay.
//!
//! When `base == 0`, the queue is bypassed entirely (see [`DelayConfig`]):
//! callers deliver messages immediately and pay nothing. This keeps the
//! production path free of artificial-latency overhead.

use std::{
    cmp::Reverse,
    collections::BinaryHeap,
    time::{Duration, Instant},
};

/// Configuration for receive-side artificial latency.
#[derive(Clone, Copy, Debug)]
pub struct DelayConfig {
    latency: Duration,
    jitter: Duration,
}

impl DelayConfig {
    /// Build a config from milliseconds. A `latency_ms` of zero disables
    /// the delay queue entirely, regardless of jitter.
    #[must_use]
    pub const fn from_millis(latency_ms: u64, jitter_ms: u64) -> Self {
        Self {
            latency: Duration::from_millis(latency_ms),
            jitter: Duration::from_millis(jitter_ms),
        }
    }

    /// Whether artificial latency is active. When false, callers should
    /// bypass the queue and deliver immediately.
    #[must_use]
    pub const fn is_enabled(&self) -> bool {
        !self.latency.is_zero()
    }

    /// Sample a delivery delay: `latency ± rand(0..=jitter)`, clamped to a
    /// non-negative duration.
    fn sample(&self) -> Duration {
        if self.jitter.is_zero() {
            return self.latency;
        }
        let jitter_ns = self.jitter.as_nanos() as u64;
        // Uniform in [-jitter, +jitter].
        let offset = fastrand::u64(0..=jitter_ns.saturating_mul(2));
        let latency_ns = self.latency.as_nanos() as u64;
        // latency + offset - jitter, saturating at zero.
        let delivery_ns = latency_ns.saturating_add(offset).saturating_sub(jitter_ns);
        Duration::from_nanos(delivery_ns)
    }
}

/// A single queued message and the instant at which it becomes
/// deliverable.
struct Delayed<T> {
    deadline: Instant,
    /// Insertion order, used only to break deadline ties deterministically
    /// so the heap has a total order. Does not affect correctness.
    seq: u64,
    message: T,
}

impl<T> PartialEq for Delayed<T> {
    fn eq(&self, other: &Self) -> bool {
        self.deadline == other.deadline && self.seq == other.seq
    }
}
impl<T> Eq for Delayed<T> {}
impl<T> PartialOrd for Delayed<T> {
    fn partial_cmp(&self, other: &Self) -> Option<std::cmp::Ordering> {
        Some(self.cmp(other))
    }
}
impl<T> Ord for Delayed<T> {
    fn cmp(&self, other: &Self) -> std::cmp::Ordering {
        // Order by deadline, then by seq, both ascending. The heap is
        // wrapped in `Reverse` by the queue to make it a min-heap.
        self.deadline
            .cmp(&other.deadline)
            .then(self.seq.cmp(&other.seq))
    }
}

/// A min-heap of messages keyed by delivery deadline.
///
/// Generic over the message type; the network layer instantiates one per
/// inbound datagram channel (snapshots on the client, commands on the
/// server).
pub struct DelayQueue<T> {
    config: DelayConfig,
    heap: BinaryHeap<Reverse<Delayed<T>>>,
    next_seq: u64,
}

impl<T> DelayQueue<T> {
    #[must_use]
    pub const fn new(config: DelayConfig) -> Self {
        Self {
            config,
            heap: BinaryHeap::new(),
            next_seq: 0,
        }
    }

    /// Enqueue a message with a freshly sampled delivery deadline.
    pub fn push(&mut self, message: T) {
        let deadline = Instant::now() + self.config.sample();
        let seq = self.next_seq;
        self.next_seq = self.next_seq.wrapping_add(1);
        self.heap.push(Reverse(Delayed {
            deadline,
            seq,
            message,
        }));
    }

    /// The earliest delivery deadline currently queued, if any. The
    /// caller uses this to arm a timer; `None` means the queue is empty
    /// and the caller should wait only for new arrivals.
    #[must_use]
    pub fn next_deadline(&self) -> Option<Instant> {
        self.heap.peek().map(|Reverse(d)| d.deadline)
    }

    /// Pop every message whose deadline is at or before `now`, in
    /// deadline order. Returns them oldest-deadline first.
    pub fn drain_ready(&mut self, now: Instant) -> Vec<T> {
        let mut ready = Vec::new();
        while let Some(Reverse(head)) = self.heap.peek() {
            if head.deadline <= now {
                if let Some(Reverse(entry)) = self.heap.pop() {
                    ready.push(entry.message);
                }
            } else {
                break;
            }
        }
        ready
    }
}
