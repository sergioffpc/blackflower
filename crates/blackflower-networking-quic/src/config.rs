use std::net::SocketAddr;
use std::num::{NonZeroU32, NonZeroUsize};
use std::sync::Arc;
use std::time::Duration;

use quinn::crypto::rustls::{QuicClientConfig, QuicServerConfig};
use rustls::pki_types::{CertificateDer, PrivateKeyDer};

use crate::QuicError;

/// Required application protocol negotiated inside TLS 1.3.
pub const ALPN_PROTOCOL: &[u8] = b"blackflower/1";
/// Ten-second full-state bootstrap deadline.
pub const BOOTSTRAP_DEADLINE: Duration = Duration::from_secs(10);

const DATAGRAM_RECEIVE_BUFFER_BYTES: usize = 2 * 1_024 * 1_024;
const DATAGRAM_SEND_BUFFER_BYTES: usize = 1_024 * 1_024;

/// Required finite limits for global Retry and validated handshakes.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub struct AdmissionLimits {
    /// Capacity of the global stateless-Retry token bucket.
    ///
    /// The bucket starts full and refills this many tokens per [`Self::window`].
    pub attempts_per_window: NonZeroU32,
    /// Interval over which the global Retry bucket refills to capacity.
    pub window: Duration,
    /// Maximum simultaneous handshakes from one validated source address.
    pub pending_per_origin: NonZeroUsize,
    /// Maximum simultaneous address-validated handshakes across the endpoint,
    /// enforced before allocating any new per-origin state.
    pub pending_global: NonZeroUsize,
    /// Maximum simultaneous established connections owned by the endpoint.
    pub connections_global: NonZeroUsize,
}

/// Consumed server TLS material; client certificates are never requested.
pub struct ServerTlsConfig {
    /// Leaf-first certificate chain signed by the service CA.
    pub certificate_chain: Vec<CertificateDer<'static>>,
    /// Private key corresponding to the leaf certificate.
    pub private_key: PrivateKeyDer<'static>,
}

/// Server endpoint configuration with no unlimited admission default.
pub struct ServerEndpointConfig {
    /// UDP address on which the dedicated server listens.
    pub bind_address: SocketAddr,
    /// Service-CA leaf certificate and key.
    pub tls: ServerTlsConfig,
    /// Explicit Retry, handshake, and established-connection admission limits.
    pub admission_limits: AdmissionLimits,
}

/// Service-CA trust set used to authenticate the dedicated server.
#[derive(Debug, Clone)]
pub struct ClientTrustRoot {
    /// Current service root. Rotation requires reconnecting with a new configuration.
    pub current: CertificateDer<'static>,
}

/// Low-level client endpoint configuration.
#[derive(Debug, Clone)]
pub struct ClientEndpointConfig {
    /// Local UDP bind address, normally an unspecified address with port zero.
    pub bind_address: SocketAddr,
    /// Dedicated server UDP address.
    pub server_address: SocketAddr,
    /// DNS name verified against the short-lived service leaf.
    pub server_name: String,
    /// Current service CA root.
    pub trust_root: ClientTrustRoot,
}

pub(crate) fn server_config(tls: ServerTlsConfig) -> Result<quinn::ServerConfig, QuicError> {
    let mut crypto =
        rustls::ServerConfig::builder_with_protocol_versions(&[&rustls::version::TLS13])
            .with_no_client_auth()
            .with_single_cert(tls.certificate_chain, tls.private_key)?;
    crypto.alpn_protocols = vec![ALPN_PROTOCOL.to_vec()];
    crypto.max_early_data_size = 0;
    let quic_crypto = QuicServerConfig::try_from(crypto)?;
    let mut config = quinn::ServerConfig::with_crypto(Arc::new(quic_crypto));
    // Quinn 0.11 drops every address-changing packet when this is false,
    // including ordinary NAT port rebinding. Keep QUIC path validation enabled;
    // the host adapter below rejects validated IP-changing active migration.
    config.migration(true);
    config.transport_config(Arc::new(server_transport()));
    Ok(config)
}

pub(crate) fn client_config(root: ClientTrustRoot) -> Result<quinn::ClientConfig, QuicError> {
    let mut root_store = rustls::RootCertStore::empty();
    root_store.add(root.current)?;
    let mut crypto =
        rustls::ClientConfig::builder_with_protocol_versions(&[&rustls::version::TLS13])
            .with_root_certificates(root_store)
            .with_no_client_auth();
    crypto.alpn_protocols = vec![ALPN_PROTOCOL.to_vec()];
    crypto.enable_early_data = false;
    crypto.resumption = rustls::client::Resumption::disabled();
    let quic_crypto = QuicClientConfig::try_from(crypto)?;
    let mut config = quinn::ClientConfig::new(Arc::new(quic_crypto));
    config.transport_config(Arc::new(client_transport()));
    Ok(config)
}

fn common_transport() -> quinn::TransportConfig {
    let mut transport = quinn::TransportConfig::default();
    transport
        .datagram_receive_buffer_size(Some(DATAGRAM_RECEIVE_BUFFER_BYTES))
        .datagram_send_buffer_size(DATAGRAM_SEND_BUFFER_BYTES)
        .keep_alive_interval(Some(Duration::from_secs(1)));
    transport
}

fn server_transport() -> quinn::TransportConfig {
    let mut transport = common_transport();
    transport
        .max_concurrent_bidi_streams(1_u8.into())
        .max_concurrent_uni_streams(0_u8.into());
    transport
}

fn client_transport() -> quinn::TransportConfig {
    let mut transport = common_transport();
    transport
        .max_concurrent_bidi_streams(0_u8.into())
        .max_concurrent_uni_streams(1_u8.into());
    transport
}

pub(crate) fn validate_alpn(connection: &quinn::Connection) -> Result<(), QuicError> {
    let handshake = connection.handshake_data().ok_or(QuicError::Alpn)?;
    let handshake = handshake
        .downcast::<quinn::crypto::rustls::HandshakeData>()
        .map_err(|_unknown| QuicError::Alpn)?;
    if handshake.protocol.as_deref() == Some(ALPN_PROTOCOL) {
        Ok(())
    } else {
        Err(QuicError::Alpn)
    }
}
