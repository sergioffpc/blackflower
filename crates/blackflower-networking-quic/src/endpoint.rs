use std::collections::BTreeMap;
use std::net::{IpAddr, SocketAddr};
use std::sync::Arc;
use std::sync::atomic::AtomicBool;
use std::time::{Duration, Instant};

use blackflower_networking::{MINIMUM_QUIC_DATAGRAM_BYTES, decode_datagram};
use bytes::Bytes;

use crate::config::{AdmissionLimits, ClientEndpointConfig, ServerEndpointConfig};
use crate::{QuicError, client_config, server_config, validate_alpn};

/// Dedicated-server Quinn endpoint with mandatory stateless Retry and bounded admission.
#[derive(Debug)]
pub struct QuicServer {
    endpoint: quinn::Endpoint,
    retry_tokens: RetryTokenBucket,
    validated_origins: ValidatedOriginLimiter,
    pending: tokio::task::JoinSet<(IpAddr, Result<quinn::Connection, quinn::ConnectionError>)>,
    retries_sent: u64,
}

impl QuicServer {
    /// Bind a dedicated-server UDP endpoint with TLS 1.3 and service-CA identity.
    pub fn bind(config: ServerEndpointConfig) -> Result<Self, QuicError> {
        validate_limits(config.admission_limits)?;
        let server = server_config(config.tls)?;
        let endpoint = quinn::Endpoint::server(server, config.bind_address)?;
        Ok(Self {
            endpoint,
            retry_tokens: RetryTokenBucket::new(config.admission_limits, Instant::now()),
            validated_origins: ValidatedOriginLimiter::new(config.admission_limits),
            pending: tokio::task::JoinSet::new(),
            retries_sent: 0,
        })
    }

    /// Return the actual UDP address, including an OS-assigned port.
    pub fn local_addr(&self) -> Result<SocketAddr, QuicError> {
        Ok(self.endpoint.local_addr()?)
    }

    /// Number of stateless Retry packets requested by this endpoint.
    #[must_use]
    pub const fn retries_sent(&self) -> u64 {
        self.retries_sent
    }

    /// Accept one address-validated and transport-compatible connection.
    pub async fn accept(&mut self) -> Result<ServerConnection, QuicError> {
        loop {
            if self.pending.is_empty() {
                let incoming = self
                    .endpoint
                    .accept()
                    .await
                    .ok_or(QuicError::EndpointClosed)?;
                self.start_handshake(incoming)?;
                continue;
            }
            tokio::select! {
                completed = self.pending.join_next() => {
                    let (origin, connection) = completed
                        .ok_or(QuicError::EndpointClosed)?
                        .map_err(|_join| QuicError::TransportTask)?;
                    self.validated_origins.finish_pending(origin);
                    let connection = connection?;
                    validate_connection(&connection)?;
                    return Ok(ServerConnection::new(connection));
                }
                incoming = self.endpoint.accept() => {
                    let incoming = incoming.ok_or(QuicError::EndpointClosed)?;
                    self.start_handshake(incoming)?;
                }
            }
        }
    }

    fn start_handshake(&mut self, incoming: quinn::Incoming) -> Result<(), QuicError> {
        if !incoming.remote_address_validated() {
            if !self.retry_tokens.try_take(Instant::now()) {
                incoming.ignore();
                return Ok(());
            }
            incoming.retry().map_err(|_error| QuicError::Retry)?;
            self.retries_sent = self.retries_sent.saturating_add(1);
            return Ok(());
        }

        let origin = incoming.remote_address().ip();
        if !self.validated_origins.begin_pending(origin) {
            incoming.refuse();
            return Ok(());
        }
        self.pending.spawn(async move { (origin, incoming.await) });
        Ok(())
    }

    /// Close the endpoint and all owned connections.
    pub fn close(&self, code: u32, reason: &[u8]) {
        self.endpoint.close(quinn::VarInt::from_u32(code), reason);
    }
}

/// Low-level Quinn client endpoint. This is not a game client or harness.
#[derive(Debug)]
pub struct QuicClient {
    endpoint: quinn::Endpoint,
    server_address: SocketAddr,
    server_name: String,
}

impl QuicClient {
    /// Bind the client UDP endpoint and install exactly the service CA roots.
    pub fn bind(config: ClientEndpointConfig) -> Result<Self, QuicError> {
        if config.server_name.is_empty() {
            return Err(QuicError::Configuration("server name is empty"));
        }
        let client = client_config(config.trust_roots)?;
        let mut endpoint = quinn::Endpoint::client(config.bind_address)?;
        endpoint.set_default_client_config(client);
        Ok(Self {
            endpoint,
            server_address: config.server_address,
            server_name: config.server_name,
        })
    }

    /// Establish a 1-RTT connection without invoking Quinn's `into_0rtt` path.
    pub async fn connect(&self) -> Result<ClientConnection, QuicError> {
        let connection = self
            .endpoint
            .connect(self.server_address, &self.server_name)?
            .await?;
        validate_connection(&connection)?;
        Ok(ClientConnection::new(connection))
    }

    /// Return the actual local UDP address.
    pub fn local_addr(&self) -> Result<SocketAddr, QuicError> {
        Ok(self.endpoint.local_addr()?)
    }

    /// Rebind the local UDP socket; Quinn validates the resulting path before use.
    pub fn rebind(&self, socket: std::net::UdpSocket) -> Result<(), QuicError> {
        self.endpoint.rebind(socket)?;
        Ok(())
    }

    /// Close the endpoint and all owned connections.
    pub fn close(&self, code: u32, reason: &[u8]) {
        self.endpoint.close(quinn::VarInt::from_u32(code), reason);
    }
}

/// Established low-level dedicated-server connection.
#[derive(Debug, Clone)]
pub struct ServerConnection {
    pub(crate) inner: quinn::Connection,
    pub(crate) control_claimed: Arc<AtomicBool>,
    pub(crate) bootstrap_active: Arc<AtomicBool>,
}

impl ServerConnection {
    fn new(inner: quinn::Connection) -> Self {
        Self {
            inner,
            control_claimed: Arc::new(AtomicBool::new(false)),
            bootstrap_active: Arc::new(AtomicBool::new(false)),
        }
    }

    /// Send one already framed and validated application DATAGRAM.
    pub fn send_datagram(&self, bytes: Vec<u8>) -> Result<(), QuicError> {
        send_datagram(&self.inner, bytes)
    }

    /// Receive one validated common-header application DATAGRAM.
    pub async fn read_datagram(&self) -> Result<Vec<u8>, QuicError> {
        read_datagram(&self.inner).await
    }

    /// Return cumulative UDP bytes reported by Quinn.
    #[must_use]
    pub fn udp_bytes(&self) -> UdpByteStats {
        udp_bytes(&self.inner)
    }

    /// Return Quinn's current smoothed RTT.
    #[must_use]
    pub fn rtt(&self) -> Duration {
        self.inner.rtt()
    }

    /// Close the application connection.
    pub fn close(&self, code: u32, reason: &[u8]) {
        self.inner.close(quinn::VarInt::from_u32(code), reason);
    }

    pub(crate) fn validate_datagram(&self, bytes: &[u8]) -> Result<(), QuicError> {
        validate_datagram(&self.inner, bytes)
    }
}

/// Established low-level client transport connection.
#[derive(Debug, Clone)]
pub struct ClientConnection {
    pub(crate) inner: quinn::Connection,
    pub(crate) control_claimed: Arc<AtomicBool>,
    pub(crate) bootstrap_active: Arc<AtomicBool>,
}

impl ClientConnection {
    fn new(inner: quinn::Connection) -> Self {
        Self {
            inner,
            control_claimed: Arc::new(AtomicBool::new(false)),
            bootstrap_active: Arc::new(AtomicBool::new(false)),
        }
    }

    /// Send one already framed and validated application DATAGRAM.
    pub fn send_datagram(&self, bytes: Vec<u8>) -> Result<(), QuicError> {
        send_datagram(&self.inner, bytes)
    }

    /// Receive one validated common-header application DATAGRAM.
    pub async fn read_datagram(&self) -> Result<Vec<u8>, QuicError> {
        read_datagram(&self.inner).await
    }

    /// Return cumulative UDP bytes reported by Quinn.
    #[must_use]
    pub fn udp_bytes(&self) -> UdpByteStats {
        udp_bytes(&self.inner)
    }

    /// Return Quinn's current smoothed RTT.
    #[must_use]
    pub fn rtt(&self) -> Duration {
        self.inner.rtt()
    }

    /// Close the application connection.
    pub fn close(&self, code: u32, reason: &[u8]) {
        self.inner.close(quinn::VarInt::from_u32(code), reason);
    }

    pub(crate) fn validate_datagram(&self, bytes: &[u8]) -> Result<(), QuicError> {
        validate_datagram(&self.inner, bytes)
    }
}

/// Cumulative UDP byte counters used for scheduler reconciliation.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub struct UdpByteStats {
    /// Bytes transmitted inside UDP datagrams.
    pub transmitted: u64,
    /// Bytes received inside UDP datagrams.
    pub received: u64,
}

fn validate_connection(connection: &quinn::Connection) -> Result<(), QuicError> {
    validate_alpn(connection)?;
    if connection
        .max_datagram_size()
        .is_none_or(|size| size < MINIMUM_QUIC_DATAGRAM_BYTES)
    {
        connection.close(quinn::VarInt::from_u32(1), b"datagram unavailable");
        return Err(QuicError::DatagramUnavailable);
    }
    Ok(())
}

fn send_datagram(connection: &quinn::Connection, bytes: Vec<u8>) -> Result<(), QuicError> {
    validate_datagram(connection, &bytes)?;
    connection.send_datagram(Bytes::from(bytes))?;
    Ok(())
}

fn validate_datagram(connection: &quinn::Connection, bytes: &[u8]) -> Result<(), QuicError> {
    let _decoded = decode_datagram(bytes)?;
    let maximum = connection
        .max_datagram_size()
        .ok_or(QuicError::DatagramUnavailable)?;
    if bytes.len() > maximum {
        return Err(QuicError::Wire(
            blackflower_networking::WireError::Oversized {
                actual: bytes.len(),
                maximum,
            },
        ));
    }
    Ok(())
}

async fn read_datagram(connection: &quinn::Connection) -> Result<Vec<u8>, QuicError> {
    let bytes = connection.read_datagram().await?;
    let _decoded = decode_datagram(&bytes)?;
    Ok(bytes.to_vec())
}

fn udp_bytes(connection: &quinn::Connection) -> UdpByteStats {
    let stats = connection.stats();
    UdpByteStats {
        transmitted: stats.udp_tx.bytes,
        received: stats.udp_rx.bytes,
    }
}

fn validate_limits(limits: AdmissionLimits) -> Result<(), QuicError> {
    if limits.window.is_zero() {
        Err(QuicError::Configuration("admission window is zero"))
    } else {
        Ok(())
    }
}

/// Constant-size global budget for stateless Retry responses.
#[derive(Debug)]
struct RetryTokenBucket {
    capacity: u32,
    available: u32,
    refill_window_nanos: u128,
    refill_remainder: u128,
    updated_at: Instant,
}

impl RetryTokenBucket {
    fn new(limits: AdmissionLimits, now: Instant) -> Self {
        let capacity = limits.attempts_per_window.get();
        Self {
            capacity,
            available: capacity,
            refill_window_nanos: limits.window.as_nanos(),
            refill_remainder: 0,
            updated_at: now,
        }
    }

    fn try_take(&mut self, now: Instant) -> bool {
        self.refill(now);
        if self.available == 0 {
            false
        } else {
            self.available -= 1;
            true
        }
    }

    fn refill(&mut self, now: Instant) {
        let Some(elapsed) = now.checked_duration_since(self.updated_at) else {
            return;
        };
        self.updated_at = now;

        if self.available == self.capacity {
            self.refill_remainder = 0;
            return;
        }

        let numerator = elapsed
            .as_nanos()
            .saturating_mul(u128::from(self.capacity))
            .saturating_add(self.refill_remainder);
        let added = numerator / self.refill_window_nanos;
        self.refill_remainder = numerator % self.refill_window_nanos;

        let missing = self.capacity - self.available;
        if added >= u128::from(missing) {
            self.available = self.capacity;
            self.refill_remainder = 0;
        } else {
            self.available += u32::try_from(added).unwrap_or(missing);
        }
    }
}

/// Per-origin state created only after QUIC has validated the source address.
#[derive(Debug)]
struct ValidatedOriginLimiter {
    pending_per_origin: usize,
    pending: BTreeMap<IpAddr, usize>,
}

impl ValidatedOriginLimiter {
    fn new(limits: AdmissionLimits) -> Self {
        Self {
            pending_per_origin: limits.pending_per_origin.get(),
            pending: BTreeMap::new(),
        }
    }

    fn begin_pending(&mut self, origin: IpAddr) -> bool {
        let pending = self.pending.entry(origin).or_default();
        if *pending >= self.pending_per_origin {
            false
        } else {
            *pending += 1;
            true
        }
    }

    fn finish_pending(&mut self, origin: IpAddr) {
        let mut remove = false;
        if let Some(pending) = self.pending.get_mut(&origin) {
            *pending = pending.saturating_sub(1);
            remove = *pending == 0;
        }
        if remove {
            let _empty = self.pending.remove(&origin);
        }
    }
}

#[cfg(test)]
#[path = "../tests/unit/endpoint.rs"]
mod tests;
