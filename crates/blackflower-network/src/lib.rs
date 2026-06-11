//! Network transport layer.
//!
//! Provides QUIC-based bidirectional communication between client and server.

use std::sync::atomic::{AtomicU64, Ordering};

use bytes::{Bytes, BytesMut};
use serde::{Deserialize, Serialize, de::DeserializeOwned};

pub mod cert;
pub mod client;
pub mod delay;
pub mod server;

/// Opaque identifier for a connected client.
///
/// Assigned by the server on connection acceptance. Stable for the
/// lifetime of the connection. Not portable across sessions.
#[derive(Clone, Copy, Debug, Default, PartialEq, Eq, Hash, Serialize, Deserialize)]
pub struct ClientId(u64);

impl ClientId {
    /// Used by the server to atomically generate identifiers.
    pub(crate) fn allocate(counter: &AtomicU64) -> Self {
        Self(counter.fetch_add(1, Ordering::Relaxed))
    }
}

impl std::fmt::Display for ClientId {
    fn fmt(&self, f: &mut std::fmt::Formatter<'_>) -> std::fmt::Result {
        write!(f, "#{}", self.0)
    }
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

/// Serialize a message with COBS framing (zero-terminated).
///
/// Suitable for QUIC streams, where the byte stream must be split
/// back into discrete messages.
pub fn encode_framed<T: Serialize>(message: &T) -> Result<Bytes, WireError> {
    let vec = postcard::to_stdvec_cobs(message).map_err(WireError::Serialize)?;
    Ok(Bytes::from(vec))
}

/// Try to decode the next COBS-framed message from `bytes`.
///
/// Returns `Ok(Some((message, consumed)))` if a full frame was decoded,
/// where `consumed` is the number of bytes consumed from the front of
/// the buffer (including the zero terminator). Returns `Ok(None)` if no
/// complete frame is present yet (the caller should read more bytes and
/// retry). Returns `Err` on a decode failure within an otherwise complete
/// frame.
#[allow(clippy::type_complexity)]
pub fn decode_framed<T: DeserializeOwned>(
    bytes: &mut [u8],
) -> Result<Option<(T, usize)>, WireError> {
    // postcard's COBS format terminates each message with a 0x00 byte.
    let Some(terminator) = bytes.iter().position(|&b| b == 0) else {
        return Ok(None);
    };

    // take_from_bytes_cobs decodes in place, mutating the frame.
    let frame = &mut bytes[..=terminator];
    let (message, _) =
        postcard::take_from_bytes_cobs::<T>(frame).map_err(WireError::Deserialize)?;

    Ok(Some((message, terminator + 1)))
}
