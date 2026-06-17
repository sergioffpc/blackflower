use std::{
    collections::VecDeque,
    sync::Arc,
    time::{Duration, Instant},
};

use arc_swap::ArcSwap;
use blackflower_protocol::Request;
use blackflower_time::Tick;
use hashbrown::HashMap;
use tracing::debug;

const NTP_SAMPLE_WINDOW: usize = 8;

// Shared between the tick thread (writer) and main thread (reader).
#[derive(Clone, Copy, Debug)]
pub(crate) struct ClockEstimate {
    // Fixed reference point established at connection time.
    pub(crate) reference_instant: Instant,
    // Estimated server tick at reference_instant.
    pub(crate) offset_ticks: f64,
}

#[derive(Clone, Copy, Debug)]
struct NtpSample {
    rtt_secs: f64,
    offset_ticks: f64,
}

pub(crate) struct ClockSync {
    tick_hz: u64,
    ping_origin: Instant,
    pending_pings: HashMap<u64, Instant>,
    samples: VecDeque<NtpSample>,
    estimate: Arc<ArcSwap<ClockEstimate>>,
}

impl ClockSync {
    pub(crate) fn new(tick_hz: u64, now: Instant, estimate: Arc<ArcSwap<ClockEstimate>>) -> Self {
        Self {
            tick_hz,
            ping_origin: now,
            pending_pings: HashMap::new(),
            samples: VecDeque::with_capacity(NTP_SAMPLE_WINDOW),
            estimate,
        }
    }

    pub(crate) fn make_ping(&mut self, now: Instant) -> Request {
        let ns = now.duration_since(self.ping_origin).as_nanos() as u64;
        self.pending_pings.insert(ns, now);
        // Evict pings with no pong after 5 s — they were dropped by the network.
        self.pending_pings
            .retain(|_, sent| now.duration_since(*sent) < Duration::from_secs(5));
        Request::Ping { client_send_ns: ns }
    }

    pub(crate) fn on_pong(&mut self, client_send_ns: u64, server_tick: u64, now: Instant) {
        let Some(send_instant) = self.pending_pings.remove(&client_send_ns) else {
            return;
        };
        let rtt_secs = now.duration_since(send_instant).as_secs_f64();

        let reference = self.estimate.load().reference_instant;
        let elapsed = now.duration_since(reference).as_secs_f64();
        // server tick at `now` ≈ server_tick_at_send + (rtt/2) * tick_hz
        let server_now = (rtt_secs / 2.0).mul_add(self.tick_hz as f64, server_tick as f64);
        let offset_ticks = elapsed.mul_add(-(self.tick_hz as f64), server_now);

        if self.samples.len() == NTP_SAMPLE_WINDOW {
            self.samples.pop_front();
        }
        self.samples.push_back(NtpSample {
            rtt_secs,
            offset_ticks,
        });
        debug!(rtt_ms = rtt_secs * 1000.0, offset_ticks, "ntp pong");

        // The sample with the lowest RTT has the least queuing bias.
        let Some(best) = self.samples.iter().min_by(|a, b| {
            a.rtt_secs
                .partial_cmp(&b.rtt_secs)
                .unwrap_or(std::cmp::Ordering::Equal)
        }) else {
            return;
        };
        self.estimate.store(Arc::new(ClockEstimate {
            reference_instant: reference,
            offset_ticks: best.offset_ticks,
        }));
    }

    /// Seed the estimate from a snapshot tick when no NTP sample has arrived yet.
    pub(crate) fn seed_from_snapshot(&self, latest_tick: Tick, now: Instant) {
        if !self.samples.is_empty() {
            return;
        }
        let reference = self.estimate.load().reference_instant;
        let elapsed = now.duration_since(reference).as_secs_f64();
        let offset_ticks = elapsed.mul_add(-(self.tick_hz as f64), latest_tick.as_f64());
        self.estimate.store(Arc::new(ClockEstimate {
            reference_instant: reference,
            offset_ticks,
        }));
    }
}
