/// Failure while configuring or operating the low-level QUIC transport.
#[derive(Debug, thiserror::Error)]
pub enum QuicError {
    /// Endpoint socket creation or query failed.
    #[error("QUIC endpoint I/O failed")]
    Io(#[from] std::io::Error),
    /// TLS certificate or key configuration is invalid.
    #[error("TLS configuration failed")]
    Tls(#[from] rustls::Error),
    /// Rustls configuration lacks the QUIC initial cipher suite.
    #[error("TLS configuration lacks the QUIC initial cipher suite")]
    InitialCipher(#[from] quinn::crypto::rustls::NoInitialCipherSuite),
    /// Client endpoint rejected connection parameters.
    #[error("QUIC connection parameters are invalid")]
    Connect(#[from] quinn::ConnectError),
    /// QUIC handshake or established connection failed.
    #[error("QUIC connection failed")]
    Connection(#[from] quinn::ConnectionError),
    /// Stateless Retry could not be emitted for an unvalidated address.
    #[error("QUIC Retry failed")]
    Retry,
    /// Negotiated ALPN is not exactly `blackflower/1`.
    #[error("QUIC ALPN mismatch")]
    Alpn,
    /// Peer does not support the mandatory 1011-byte QUIC DATAGRAM size.
    #[error("QUIC DATAGRAM capacity is unavailable or below 1011 bytes")]
    DatagramUnavailable,
    /// Application datagram failed common-header validation.
    #[error(transparent)]
    Wire(#[from] blackflower_networking::WireError),
    /// Quinn refused an application datagram.
    #[error("QUIC DATAGRAM send failed")]
    SendDatagram(#[from] quinn::SendDatagramError),
    /// Reliable stream write failed.
    #[error("QUIC stream write failed")]
    Write(#[from] quinn::WriteError),
    /// Reliable stream was already closed locally.
    #[error("QUIC stream is closed")]
    ClosedStream(#[from] quinn::ClosedStream),
    /// Exact reliable stream read failed.
    #[error("QUIC stream exact read failed")]
    ReadExact(#[from] quinn::ReadExactError),
    /// Bounded reliable stream read failed.
    #[error("QUIC stream bounded read failed")]
    ReadToEnd(#[from] quinn::ReadToEndError),
    /// Bootstrap exceeded its ten-second transfer deadline.
    #[error("state bootstrap transfer deadline expired")]
    BootstrapDeadline,
    /// Peer opened a stream with a forbidden role or repeated the unique stream.
    #[error("peer violated the QUIC stream-role contract")]
    StreamRole,
    /// Per-origin admission limiting rejected the connection attempt.
    #[error("per-origin admission limit exceeded")]
    AdmissionLimited,
    /// Endpoint was closed while waiting for a connection.
    #[error("QUIC endpoint is closed")]
    EndpointClosed,
    /// A Tokio transport task stopped before returning its handshake result.
    #[error("QUIC transport task stopped")]
    TransportTask,
    /// A bounded host queue is full.
    #[error("bounded host transport queue is full")]
    QueueFull,
    /// A host-facing queue mutex was poisoned.
    #[error("host transport queue is unavailable")]
    QueueUnavailable,
    /// Endpoint configuration contains an invalid finite limit.
    #[error("invalid QUIC endpoint configuration: {0}")]
    Configuration(&'static str),
}
