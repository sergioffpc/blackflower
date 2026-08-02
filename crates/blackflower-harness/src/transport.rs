use std::error::Error as StdError;
use std::net::SocketAddr;

use blackflower_networking::StateBootstrapHeader;
use blackflower_networking_quic::{
    BootstrapTransfer, ClientNetworkHandle, NetworkEvent, QuicError,
};

/// Inbound transport fact consumed by the shared client harness.
#[derive(Debug, Clone, PartialEq, Eq)]
pub enum ClientTransportEvent {
    /// One reliable session-control frame.
    SessionControl(Vec<u8>),
    /// One validated application datagram.
    Datagram(Vec<u8>),
    /// One complete reliable full-state transfer.
    Bootstrap {
        /// Canonical bootstrap header.
        header: StateBootstrapHeader,
        /// Exact uncompressed canonical snapshot bytes.
        body: Vec<u8>,
    },
    /// The validated remote UDP path changed without changing address family.
    PathChanged {
        /// Previously validated peer address.
        previous: SocketAddr,
        /// Newly validated peer address.
        current: SocketAddr,
    },
    /// The transport stopped and the application session must close or reconnect.
    TransportStopped,
}

/// Bounded transport operations required by a human or headless client.
pub trait ClientTransport {
    /// Concrete transport failure.
    type Error: StdError + Send + Sync + 'static;

    /// Queue one reliable session-control frame.
    fn send_control(&mut self, frame: Vec<u8>) -> Result<(), Self::Error>;

    /// Replace the unsent input datagram with the newest exact value.
    fn set_latest_input(&mut self, datagram: Vec<u8>) -> Result<(), Self::Error>;

    /// Poll one transport fact without blocking.
    fn receive(&mut self) -> Result<Option<ClientTransportEvent>, Self::Error>;
}

impl ClientTransport for ClientNetworkHandle {
    type Error = QuicError;

    fn send_control(&mut self, frame: Vec<u8>) -> Result<(), Self::Error> {
        self.try_send_control(frame)
    }

    fn set_latest_input(&mut self, datagram: Vec<u8>) -> Result<(), Self::Error> {
        ClientNetworkHandle::set_latest_input(self, datagram)
    }

    fn receive(&mut self) -> Result<Option<ClientTransportEvent>, Self::Error> {
        self.try_receive().map(|event| event.map(Into::into))
    }
}

impl From<NetworkEvent> for ClientTransportEvent {
    fn from(event: NetworkEvent) -> Self {
        match event {
            NetworkEvent::SessionControl(frame) => Self::SessionControl(frame),
            NetworkEvent::Datagram(datagram) => Self::Datagram(datagram),
            NetworkEvent::Bootstrap(BootstrapTransfer { header, body }) => {
                Self::Bootstrap { header, body }
            }
            NetworkEvent::PathChanged { previous, current } => {
                Self::PathChanged { previous, current }
            }
            NetworkEvent::TransportStopped => Self::TransportStopped,
        }
    }
}
