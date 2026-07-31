use std::collections::VecDeque;

/// Side of the in-memory client/server datagram link.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum HarnessEndpoint {
    /// Client-facing receive queue.
    Client,
    /// Server-facing receive queue.
    Server,
}

/// Bounded in-memory transport failure.
#[derive(Debug, Clone, Copy, PartialEq, Eq, thiserror::Error)]
pub enum HarnessError {
    /// A queue reached its configured packet capacity.
    #[error("in-memory datagram queue is full")]
    Full,
    /// Empty packets are never valid datagrams.
    #[error("in-memory datagram is empty")]
    Empty,
}

/// Bounded deterministic harness; it deliberately implements no sockets or clocks.
#[derive(Debug)]
pub struct InMemoryDatagramHarness {
    capacity: usize,
    client: VecDeque<Vec<u8>>,
    server: VecDeque<Vec<u8>>,
}

impl InMemoryDatagramHarness {
    /// Allocate queues for a fixed packet count.
    #[must_use]
    pub fn new(capacity: usize) -> Self {
        Self {
            capacity,
            client: VecDeque::with_capacity(capacity),
            server: VecDeque::with_capacity(capacity),
        }
    }

    /// Send a datagram to one endpoint.
    pub fn send(&mut self, endpoint: HarnessEndpoint, bytes: Vec<u8>) -> Result<(), HarnessError> {
        if bytes.is_empty() {
            return Err(HarnessError::Empty);
        }
        let queue = match endpoint {
            HarnessEndpoint::Client => &mut self.client,
            HarnessEndpoint::Server => &mut self.server,
        };
        if queue.len() >= self.capacity {
            return Err(HarnessError::Full);
        }
        queue.push_back(bytes);
        Ok(())
    }

    /// Receive the oldest datagram for one endpoint.
    pub fn receive(&mut self, endpoint: HarnessEndpoint) -> Option<Vec<u8>> {
        match endpoint {
            HarnessEndpoint::Client => self.client.pop_front(),
            HarnessEndpoint::Server => self.server.pop_front(),
        }
    }
}
