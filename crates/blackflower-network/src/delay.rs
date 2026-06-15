use std::{
    cmp::Reverse,
    collections::BinaryHeap,
    time::{Duration, Instant},
};

#[derive(Clone, Copy, Debug)]
pub struct DelayConfig {
    latency: Duration,
    jitter: Duration,
}

impl DelayConfig {
    #[must_use]
    pub const fn from_millis(latency_ms: u64, jitter_ms: u64) -> Self {
        Self {
            latency: Duration::from_millis(latency_ms),
            jitter: Duration::from_millis(jitter_ms),
        }
    }

    #[must_use]
    pub const fn is_enabled(&self) -> bool {
        !self.latency.is_zero()
    }

    fn sample(&self) -> Duration {
        if self.jitter.is_zero() {
            return self.latency;
        }
        let jitter_ns = self.jitter.as_nanos() as u64;
        let offset = fastrand::u64(0..=jitter_ns.saturating_mul(2));
        let latency_ns = self.latency.as_nanos() as u64;
        let delivery_ns = latency_ns.saturating_add(offset).saturating_sub(jitter_ns);
        Duration::from_nanos(delivery_ns)
    }
}

struct Delayed<T> {
    deadline: Instant,
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
        self.deadline
            .cmp(&other.deadline)
            .then(self.seq.cmp(&other.seq))
    }
}

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

    #[must_use]
    pub fn next_deadline(&self) -> Option<Instant> {
        self.heap.peek().map(|Reverse(d)| d.deadline)
    }

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
