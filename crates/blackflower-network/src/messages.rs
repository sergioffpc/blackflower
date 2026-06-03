//! Wire message types exchanged between client and server.

use bytes::{Bytes, BytesMut};
use serde::{Deserialize, Serialize};

/// Messages sent from the client to the server.
#[derive(Debug, Clone, Serialize, Deserialize)]
pub enum Control {
    /// Tell the server we are ready to receive snapshots.
    Connect,
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
