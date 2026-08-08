use std::collections::BTreeMap;
use std::time::Duration;

use bytes::{Bytes, BytesMut};

#[derive(Debug, Clone, Copy)]
struct ImpairmentProfile {
    one_way_delay: Duration,
    jitter_steps: u64,
    lose_every: u64,
    duplicate_every: u64,
    reorder_every: u64,
    mtu: usize,
    outage: Option<(Duration, Duration)>,
}

#[derive(Debug, Default, PartialEq, Eq)]
struct ProxyCounters {
    lost: u64,
    duplicated: u64,
    mtu_dropped: u64,
    outage_dropped: u64,
}

#[derive(Debug)]
struct UdpImpairmentProxy {
    profile: ImpairmentProfile,
    sequence: u64,
    insertion: u64,
    queued: BTreeMap<(Duration, u64), Bytes>,
    counters: ProxyCounters,
}

impl UdpImpairmentProxy {
    fn new(profile: ImpairmentProfile) -> Self {
        Self {
            profile,
            sequence: 0,
            insertion: 0,
            queued: BTreeMap::new(),
            counters: ProxyCounters::default(),
        }
    }

    fn receive(&mut self, now: Duration, packet: Bytes) {
        self.sequence = self.sequence.saturating_add(1);
        if packet.len() > self.profile.mtu {
            self.counters.mtu_dropped = self.counters.mtu_dropped.saturating_add(1);
            return;
        }
        if self
            .profile
            .outage
            .is_some_and(|(start, end)| now >= start && now < end)
        {
            self.counters.outage_dropped = self.counters.outage_dropped.saturating_add(1);
            return;
        }
        if self.profile.lose_every != 0 && self.sequence.is_multiple_of(self.profile.lose_every) {
            self.counters.lost = self.counters.lost.saturating_add(1);
            return;
        }
        let jitter = Duration::from_millis(self.sequence % (self.profile.jitter_steps + 1));
        let reorder = if self.profile.reorder_every != 0
            && self.sequence.is_multiple_of(self.profile.reorder_every)
        {
            self.profile.one_way_delay
        } else {
            Duration::ZERO
        };
        let delivery = now
            .saturating_add(self.profile.one_way_delay)
            .saturating_add(jitter)
            .saturating_add(reorder);
        self.queue(delivery, packet.clone());
        if self.profile.duplicate_every != 0
            && self.sequence.is_multiple_of(self.profile.duplicate_every)
        {
            self.counters.duplicated = self.counters.duplicated.saturating_add(1);
            self.queue(delivery.saturating_add(Duration::from_millis(1)), packet);
        }
    }

    fn drain(&mut self, now: Duration) -> Vec<Bytes> {
        let keys = self
            .queued
            .range(..=(now, u64::MAX))
            .map(|(key, _packet)| *key)
            .collect::<Vec<_>>();
        keys.into_iter()
            .filter_map(|key| self.queued.remove(&key))
            .collect()
    }

    fn queue(&mut self, delivery: Duration, packet: Bytes) {
        self.insertion = self.insertion.saturating_add(1);
        self.queued.insert((delivery, self.insertion), packet);
    }
}

#[test]
fn deterministic_udp_proxy_covers_every_network_gate_impairment() {
    let mut proxy = UdpImpairmentProxy::new(ImpairmentProfile {
        one_way_delay: Duration::from_millis(30),
        jitter_steps: 10,
        lose_every: 5,
        duplicate_every: 3,
        reorder_every: 2,
        mtu: 1_200,
        outage: Some((Duration::from_millis(100), Duration::from_millis(200))),
    });
    proxy.receive(Duration::ZERO, Bytes::from_static(&[1]));
    proxy.receive(Duration::ZERO, Bytes::from_static(&[2]));
    proxy.receive(Duration::ZERO, Bytes::from_static(&[3]));
    proxy.receive(Duration::ZERO, Bytes::from_static(&[4]));
    proxy.receive(Duration::ZERO, Bytes::from_static(&[5]));
    proxy.receive(Duration::ZERO, BytesMut::zeroed(1_201).freeze());
    proxy.receive(Duration::from_millis(150), Bytes::from_static(&[7]));

    assert!(proxy.drain(Duration::from_millis(20)).is_empty());
    assert_eq!(proxy.drain(Duration::from_millis(100)).len(), 5);
    assert_eq!(
        proxy.counters,
        ProxyCounters {
            lost: 1,
            duplicated: 1,
            mtu_dropped: 1,
            outage_dropped: 1,
        }
    );
}
