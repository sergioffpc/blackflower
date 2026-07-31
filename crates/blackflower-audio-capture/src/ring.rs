use std::sync::atomic::{AtomicU32, AtomicU64, AtomicUsize, Ordering};

#[derive(Debug)]
pub(crate) struct SampleRing {
    samples: Box<[AtomicU32]>,
    read: AtomicUsize,
    write: AtomicUsize,
    dropped: AtomicU64,
}

impl SampleRing {
    pub(crate) fn new(capacity: usize) -> Self {
        let samples = (0..capacity.max(2))
            .map(|_index| AtomicU32::new(0))
            .collect::<Vec<_>>()
            .into_boxed_slice();
        Self {
            samples,
            read: AtomicUsize::new(0),
            write: AtomicUsize::new(0),
            dropped: AtomicU64::new(0),
        }
    }

    pub(crate) fn push(&self, sample: f32) {
        let write = self.write.load(Ordering::Relaxed);
        let next = (write + 1) % self.samples.len();
        if next == self.read.load(Ordering::Acquire) {
            self.dropped.fetch_add(1, Ordering::Relaxed);
            return;
        }
        self.samples[write].store(sample.to_bits(), Ordering::Relaxed);
        self.write.store(next, Ordering::Release);
    }

    pub(crate) fn pop(&self) -> Option<f32> {
        let read = self.read.load(Ordering::Relaxed);
        if read == self.write.load(Ordering::Acquire) {
            return None;
        }
        let sample = f32::from_bits(self.samples[read].load(Ordering::Relaxed));
        self.read
            .store((read + 1) % self.samples.len(), Ordering::Release);
        Some(sample)
    }

    pub(crate) fn dropped(&self) -> u64 {
        self.dropped.load(Ordering::Relaxed)
    }
}
