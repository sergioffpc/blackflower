//! Wire message types exchanged between client and server.
//!
//! Serialized with [`postcard`]. Format is binary, length-prefixed
//! (when sent over QUIC streams) or raw (when sent over QUIC datagrams).

use bytes::{Bytes, BytesMut};
use serde::{Deserialize, Serialize};

/// Messages sent from the client to the server.
#[derive(Debug, Clone, Serialize, Deserialize)]
pub enum ClientToServer {
    /// Initial handshake message sent after connection is established.
    Hello,
}

/// Messages sent from the server to the client.
#[derive(Debug, Clone, Serialize, Deserialize)]
pub enum ServerToClient {
    /// Acknowledgment of a `Hello`.
    Ack,
}

/// Errors raised by the wire encoding/decoding layer.
#[derive(Debug, thiserror::Error)]
pub enum WireError {
    #[error("serialization failed: {0}")]
    Serialize(postcard::Error),

    #[error("deserialization failed: {0}")]
    Deserialize(postcard::Error),
}

/// Serialize a message into a reusable buffer (zero-copy friendly).
pub fn encode<T: Serialize>(message: &T) -> Result<Bytes, WireError> {
    let buf = postcard::to_extend(message, BytesMut::with_capacity(1024))
        .map_err(WireError::Serialize)?;
    Ok(buf.freeze())
}

/// Deserialize a message from a byte slice.
pub fn decode<'a, T: Deserialize<'a>>(bytes: &'a [u8]) -> Result<T, WireError> {
    postcard::from_bytes(bytes).map_err(WireError::Deserialize)
}
