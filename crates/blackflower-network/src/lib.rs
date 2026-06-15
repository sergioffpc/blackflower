use bytes::{Bytes, BytesMut};
use serde::{Deserialize, Serialize, de::DeserializeOwned};

pub mod cert;
pub mod client;
pub mod connection;
pub mod delay;
pub mod server;

#[derive(Debug, thiserror::Error)]
pub enum WireError {
    #[error("serialization failed: {0}")]
    Serialize(postcard::Error),

    #[error("deserialization failed: {0}")]
    Deserialize(postcard::Error),
}

pub fn encode<T: Serialize>(message: &T) -> Result<Bytes, WireError> {
    let buf = postcard::to_extend(message, BytesMut::with_capacity(1024))
        .map_err(WireError::Serialize)?;
    Ok(buf.freeze())
}

pub fn decode<'a, T: Deserialize<'a>>(bytes: &'a [u8]) -> Result<T, WireError> {
    postcard::from_bytes(bytes).map_err(WireError::Deserialize)
}

pub fn encode_framed<T: Serialize>(message: &T) -> Result<Bytes, WireError> {
    let vec = postcard::to_stdvec_cobs(message).map_err(WireError::Serialize)?;
    Ok(Bytes::from(vec))
}

#[allow(clippy::type_complexity)]
pub fn decode_framed<T: DeserializeOwned>(
    bytes: &mut [u8],
) -> Result<Option<(T, usize)>, WireError> {
    let Some(terminator) = bytes.iter().position(|&b| b == 0) else {
        return Ok(None);
    };

    let frame = &mut bytes[..=terminator];
    let (message, _) =
        postcard::take_from_bytes_cobs::<T>(frame).map_err(WireError::Deserialize)?;

    Ok(Some((message, terminator + 1)))
}
