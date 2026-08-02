use std::sync::atomic::Ordering;

use blackflower_networking::{
    MAX_BOOTSTRAP_BYTES, MAX_CONTROL_MESSAGE_BYTES, STATE_BOOTSTRAP_HEADER_BYTES,
    StateBootstrapHeader, StreamKind, WireError, decode_frame, decode_state_bootstrap_header,
    decode_stream_preamble, encode_state_bootstrap_header, encode_stream_preamble,
};

use crate::config::BOOTSTRAP_DEADLINE;
use crate::{ClientConnection, QuicError, ServerConnection};

/// The unique long-lived client-initiated bidirectional control stream.
#[derive(Debug)]
pub struct SessionControlStream {
    pub(crate) send: quinn::SendStream,
    pub(crate) receive: quinn::RecvStream,
}

impl SessionControlStream {
    /// Send one exact kind-and-varint framed control message.
    pub async fn send(&mut self, frame: &[u8]) -> Result<(), QuicError> {
        let _decoded = decode_frame(frame, MAX_CONTROL_MESSAGE_BYTES)?;
        self.send.write_all(frame).await?;
        Ok(())
    }

    /// Receive one exact bounded control frame from the stream.
    pub async fn receive(&mut self) -> Result<Vec<u8>, QuicError> {
        read_control_frame(&mut self.receive).await
    }

    /// Finish the sending direction during graceful application close.
    pub fn finish(&mut self) -> Result<(), QuicError> {
        self.send.finish()?;
        Ok(())
    }
}

/// One complete uncompressed full-state transfer.
#[derive(Debug, Clone, PartialEq, Eq)]
pub struct BootstrapTransfer {
    /// Validated fixed bootstrap header.
    pub header: StateBootstrapHeader,
    /// Exact uncompressed canonical snapshot bytes.
    pub body: Vec<u8>,
}

impl ClientConnection {
    /// Open the unique client-initiated `SessionControl` stream.
    pub async fn open_session_control(&self) -> Result<SessionControlStream, QuicError> {
        claim_once(&self.control_claimed)?;
        let result = open_control(&self.inner).await;
        if result.is_err() {
            self.control_claimed.store(false, Ordering::Release);
        }
        result
    }

    /// Receive the next single active server-initiated bootstrap stream.
    pub async fn receive_bootstrap(&self) -> Result<BootstrapTransfer, QuicError> {
        claim_once(&self.bootstrap_active)?;
        let result = receive_bootstrap(&self.inner).await;
        self.bootstrap_active.store(false, Ordering::Release);
        result
    }
}

impl ServerConnection {
    /// Accept the unique client-initiated `SessionControl` stream.
    pub async fn accept_session_control(&self) -> Result<SessionControlStream, QuicError> {
        claim_once(&self.control_claimed)?;
        let result = accept_control(&self.inner).await;
        if result.is_err() {
            self.control_claimed.store(false, Ordering::Release);
        }
        result
    }

    /// Send one uncompressed full-state bootstrap on a dedicated uni stream.
    pub async fn send_bootstrap(
        &self,
        header: StateBootstrapHeader,
        body: &[u8],
    ) -> Result<(), QuicError> {
        claim_once(&self.bootstrap_active)?;
        let result = send_bootstrap(&self.inner, header, body).await;
        self.bootstrap_active.store(false, Ordering::Release);
        result
    }
}

async fn open_control(connection: &quinn::Connection) -> Result<SessionControlStream, QuicError> {
    let (mut send, receive) = connection.open_bi().await?;
    send.write_all(&encode_stream_preamble(StreamKind::SessionControl))
        .await?;
    Ok(SessionControlStream { send, receive })
}

async fn accept_control(connection: &quinn::Connection) -> Result<SessionControlStream, QuicError> {
    let (send, mut receive) = connection.accept_bi().await?;
    let mut preamble = [0_u8; 4];
    receive.read_exact(&mut preamble).await?;
    if decode_stream_preamble(&preamble)? != StreamKind::SessionControl {
        return Err(QuicError::StreamRole);
    }
    Ok(SessionControlStream { send, receive })
}

async fn send_bootstrap(
    connection: &quinn::Connection,
    header: StateBootstrapHeader,
    body: &[u8],
) -> Result<(), QuicError> {
    let declared =
        usize::try_from(header.body_length).map_err(|_error| WireError::IntegerOutOfRange)?;
    if body.len() != declared {
        return Err(QuicError::Wire(if body.len() < declared {
            WireError::Truncated
        } else {
            WireError::Trailing
        }));
    }
    let fixed = encode_state_bootstrap_header(header)?;
    let transfer = async {
        let mut send = connection.open_uni().await?;
        send.write_all(&encode_stream_preamble(StreamKind::StateBootstrap))
            .await?;
        send.write_all(&fixed).await?;
        send.write_all(body).await?;
        send.finish()?;
        Ok::<(), QuicError>(())
    };
    tokio::time::timeout(BOOTSTRAP_DEADLINE, transfer)
        .await
        .map_err(|_elapsed| QuicError::BootstrapDeadline)??;
    Ok(())
}

async fn receive_bootstrap(connection: &quinn::Connection) -> Result<BootstrapTransfer, QuicError> {
    let transfer = async {
        let mut receive = connection.accept_uni().await?;
        let mut preamble = [0_u8; 4];
        receive.read_exact(&mut preamble).await?;
        if decode_stream_preamble(&preamble)? != StreamKind::StateBootstrap {
            return Err(QuicError::StreamRole);
        }
        let mut fixed = [0_u8; STATE_BOOTSTRAP_HEADER_BYTES];
        receive.read_exact(&mut fixed).await?;
        let header = decode_state_bootstrap_header(&fixed)?;
        let body = receive.read_to_end(MAX_BOOTSTRAP_BYTES).await?;
        let declared =
            usize::try_from(header.body_length).map_err(|_error| WireError::IntegerOutOfRange)?;
        if body.len() != declared {
            return Err(QuicError::Wire(if body.len() < declared {
                WireError::Truncated
            } else {
                WireError::Trailing
            }));
        }
        Ok(BootstrapTransfer { header, body })
    };
    tokio::time::timeout(BOOTSTRAP_DEADLINE, transfer)
        .await
        .map_err(|_elapsed| QuicError::BootstrapDeadline)?
}

pub(crate) async fn read_control_frame(
    receive: &mut quinn::RecvStream,
) -> Result<Vec<u8>, QuicError> {
    let mut kind = [0_u8; 1];
    receive.read_exact(&mut kind).await?;
    let (length_bytes, length) = read_varint(receive).await?;
    if length > MAX_CONTROL_MESSAGE_BYTES {
        return Err(QuicError::Wire(WireError::Oversized {
            actual: length,
            maximum: MAX_CONTROL_MESSAGE_BYTES,
        }));
    }
    let mut payload = vec![0_u8; length];
    receive.read_exact(&mut payload).await?;
    let mut frame = Vec::with_capacity(1 + length_bytes.len() + payload.len());
    frame.push(kind[0]);
    frame.extend_from_slice(&length_bytes);
    frame.extend_from_slice(&payload);
    let _decoded = decode_frame(&frame, MAX_CONTROL_MESSAGE_BYTES)?;
    Ok(frame)
}

async fn read_varint(receive: &mut quinn::RecvStream) -> Result<(Vec<u8>, usize), QuicError> {
    let mut first = [0_u8; 1];
    receive.read_exact(&mut first).await?;
    let width = 1_usize << usize::from(first[0] >> 6);
    let mut bytes = vec![0_u8; width];
    bytes[0] = first[0];
    if width > 1 {
        receive.read_exact(&mut bytes[1..]).await?;
    }
    let mut value = u64::from(bytes[0] & 0x3f);
    for &byte in &bytes[1..] {
        value = (value << 8) | u64::from(byte);
    }
    let length = usize::try_from(value).map_err(|_error| WireError::IntegerOutOfRange)?;
    Ok((bytes, length))
}

fn claim_once(claimed: &std::sync::atomic::AtomicBool) -> Result<(), QuicError> {
    claimed
        .compare_exchange(false, true, Ordering::AcqRel, Ordering::Acquire)
        .map(|_previous| ())
        .map_err(|_previous| QuicError::StreamRole)
}
