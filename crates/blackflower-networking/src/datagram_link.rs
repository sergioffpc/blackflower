use std::collections::VecDeque;

/// Side of the in-memory client/server datagram link.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum DatagramLinkEndpoint {
    /// Client-facing receive queue.
    Client,
    /// Server-facing receive queue.
    Server,
}

/// Bounded in-memory datagram-link failure.
#[derive(Debug, Clone, Copy, PartialEq, Eq, thiserror::Error)]
pub enum DatagramLinkError {
    /// A queue reached its configured packet capacity.
    #[error("in-memory datagram queue is full")]
    Full,
    /// Empty packets are never valid datagrams.
    #[error("in-memory datagram is empty")]
    Empty,
}

/// Bounded deterministic link; it deliberately implements no sockets or clocks.
#[derive(Debug)]
pub struct InMemoryDatagramLink {
    capacity: usize,
    client: VecDeque<Vec<u8>>,
    server: VecDeque<Vec<u8>>,
}

impl InMemoryDatagramLink {
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
    pub fn send(
        &mut self,
        endpoint: DatagramLinkEndpoint,
        bytes: Vec<u8>,
    ) -> Result<(), DatagramLinkError> {
        if bytes.is_empty() {
            return Err(DatagramLinkError::Empty);
        }
        let queue = match endpoint {
            DatagramLinkEndpoint::Client => &mut self.client,
            DatagramLinkEndpoint::Server => &mut self.server,
        };
        if queue.len() >= self.capacity {
            return Err(DatagramLinkError::Full);
        }
        queue.push_back(bytes);
        Ok(())
    }

    /// Receive the oldest datagram for one endpoint.
    pub fn receive(&mut self, endpoint: DatagramLinkEndpoint) -> Option<Vec<u8>> {
        match endpoint {
            DatagramLinkEndpoint::Client => self.client.pop_front(),
            DatagramLinkEndpoint::Server => self.server.pop_front(),
        }
    }
}
