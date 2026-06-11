//! Client-side QUIC endpoint.
//!
//! Synchronous public API; internally drives a single-threaded tokio runtime.

use std::{net::SocketAddr, sync::Arc, thread::JoinHandle};

use anyhow::Context;
use crossbeam_channel::TrySendError;
use quinn::{ClientConfig, Connection, ConnectionError, Endpoint, RecvStream, SendStream};
use serde::{Serialize, de::DeserializeOwned};
use tokio::sync::{mpsc, oneshot};
use tracing::{debug, error, info, warn};

use crate::{
    cert::SkipServerVerification,
    decode, decode_framed,
    delay::{DelayConfig, DelayQueue},
    encode, encode_framed,
};

const COMMAND_QUEUE_CAPACITY: usize = 8;
const SNAPSHOT_QUEUE_CAPACITY: usize = 8;
const REQUEST_QUEUE_CAPACITY: usize = 32;
const EVENT_QUEUE_CAPACITY: usize = 32;

pub struct ClientHandle<C, S, R, E> {
    command_tx: mpsc::Sender<C>,
    snapshot_rx: crossbeam_channel::Receiver<S>,
    request_tx: mpsc::Sender<R>,
    event_rx: crossbeam_channel::Receiver<E>,
    shutdown_tx: Option<oneshot::Sender<()>>,
    join_handle: Option<JoinHandle<()>>,
}

impl<C, S, R, E> ClientHandle<C, S, R, E> {
    pub fn try_recv_snapshots(&self) -> impl Iterator<Item = S> + '_ {
        self.snapshot_rx.try_iter()
    }

    pub fn try_send_command(&self, command: C)
    where
        C: Send + Sync + 'static,
    {
        match self.command_tx.try_send(command) {
            Ok(()) => {}
            Err(mpsc::error::TrySendError::Full(_)) => {
                warn!("command queue full; dropping input");
            }
            Err(mpsc::error::TrySendError::Closed(_)) => {
                warn!("command channel closed");
            }
        }
    }

    pub fn try_recv_events(&self) -> impl Iterator<Item = E> + '_ {
        self.event_rx.try_iter()
    }

    pub fn try_send_request(&self, request: R)
    where
        R: Send + Sync + 'static,
    {
        match self.request_tx.try_send(request) {
            Ok(()) => {}
            Err(mpsc::error::TrySendError::Full(_)) => {
                warn!("request queue full; dropping request");
            }
            Err(mpsc::error::TrySendError::Closed(_)) => {
                warn!("request channel closed");
            }
        }
    }
}

impl<C, S, R, E> Drop for ClientHandle<C, S, R, E> {
    fn drop(&mut self) {
        if let Some(tx) = self.shutdown_tx.take() {
            tx.send(()).ok();
        }
        if let Some(handle) = self.join_handle.take()
            && let Err(err) = handle.join()
        {
            error!(error = ?err, "network client thread");
        }
    }
}

pub fn connect<C, S, R, E>(
    server_addr: SocketAddr,
    delay: DelayConfig,
) -> anyhow::Result<ClientHandle<C, S, R, E>>
where
    C: Serialize + Send + 'static,
    S: DeserializeOwned + Send + Sync + 'static,
    R: Serialize + Send + 'static,
    E: DeserializeOwned + Send + 'static,
{
    rustls::crypto::ring::default_provider()
        .install_default()
        .ok();

    let (command_tx, command_rx) = mpsc::channel::<C>(COMMAND_QUEUE_CAPACITY);
    let (snapshot_tx, snapshot_rx) = crossbeam_channel::bounded::<S>(SNAPSHOT_QUEUE_CAPACITY);
    let (request_tx, request_rx) = mpsc::channel::<R>(REQUEST_QUEUE_CAPACITY);
    let (event_tx, event_rx) = crossbeam_channel::bounded::<E>(EVENT_QUEUE_CAPACITY);
    let (shutdown_tx, shutdown_rx) = oneshot::channel::<()>();

    let join_handle = std::thread::Builder::new()
        .name("blackflower-net::client".to_owned())
        .spawn(move || {
            if let Err(e) = start(
                server_addr,
                delay,
                ClientChannelGroupDescriptor {
                    command_rx,
                    snapshot_tx,
                    request_rx,
                    event_tx,
                    shutdown_rx,
                },
            ) {
                error!(error = %e, "connection failed");
            }
        })
        .context("spawning network client thread")?;

    Ok(ClientHandle {
        command_tx,
        snapshot_rx,
        request_tx,
        event_rx,
        shutdown_tx: Some(shutdown_tx),
        join_handle: Some(join_handle),
    })
}

struct ClientChannelGroupDescriptor<C, S, R, E> {
    command_rx: mpsc::Receiver<C>,
    snapshot_tx: crossbeam_channel::Sender<S>,
    request_rx: mpsc::Receiver<R>,
    event_tx: crossbeam_channel::Sender<E>,
    shutdown_rx: oneshot::Receiver<()>,
}

fn start<C, S, R, E>(
    server_addr: SocketAddr,
    delay: DelayConfig,
    mut desc: ClientChannelGroupDescriptor<C, S, R, E>,
) -> anyhow::Result<()>
where
    C: Serialize,
    S: DeserializeOwned + Send + Sync,
    R: Serialize,
    E: DeserializeOwned,
{
    let runtime = tokio::runtime::Builder::new_current_thread()
        .enable_all()
        .build()
        .context("building client async runtime")?;

    runtime.block_on(async move {
        let mut rustls_config = rustls::ClientConfig::builder()
            .dangerous()
            .with_custom_certificate_verifier(SkipServerVerification::new())
            .with_no_client_auth();

        // ALPN: any string works as long as client and server agree. Pick a name.
        rustls_config.alpn_protocols = vec![b"blackflower/0".to_vec()];

        let quic_config = quinn::crypto::rustls::QuicClientConfig::try_from(rustls_config)
            .context("converting rustls config to QUIC")?;
        let client_config = ClientConfig::new(Arc::new(quic_config));

        // Bind to an ephemeral local port on the unspecified address.
        let mut endpoint =
            Endpoint::client("0.0.0.0:0".parse()?).context("binding client socket")?;
        endpoint.set_default_client_config(client_config);

        info!(server = %server_addr, "connecting");

        let connection = endpoint
            .connect(server_addr, "localhost")
            .context("starting connection")?
            .await
            .context("connection handshake failed")?;
        info!(server = %server_addr, "connected");

        let (send, recv) = connection
            .open_bi()
            .await
            .context("opening control stream")?;

        tokio::select! {
            biased;

            _ = desc.shutdown_rx => {
                info!("client received shutdown signal");
            }
            () = send_commands(&connection, &mut desc.command_rx) => {}
            () = recv_snapshots(&connection, delay, &desc.snapshot_tx) => {}
            () = send_requests(send, &mut desc.request_rx) => {}
            () = recv_events(recv, &desc.event_tx) => {}
        }

        connection.close(0_u32.into(), b"client done");
        endpoint.wait_idle().await;

        Ok(())
    })
}

async fn send_commands<C>(connection: &Connection, command_rx: &mut mpsc::Receiver<C>)
where
    C: Serialize,
{
    while let Some(command) = command_rx.recv().await {
        match encode::<C>(&command) {
            Ok(bytes) => {
                if let Err(e) = connection.send_datagram(bytes) {
                    error!(error = %e, "sending command datagram");
                    break;
                }
            }
            Err(e) => warn!(error = %e, "encoding command failed"),
        }
    }
}

async fn recv_snapshots<S>(
    connection: &Connection,
    delay: DelayConfig,
    snapshot_tx: &crossbeam_channel::Sender<S>,
) where
    S: DeserializeOwned + Send + Sync,
{
    let mut queue = DelayQueue::new(delay);

    loop {
        // Arm a timer for the head of the delay queue. When the queue is
        // empty there is no deadline, so this future never completes and
        // the select reacts only to new datagrams.
        let deliver_tick = async {
            match queue.next_deadline() {
                Some(deadline) => tokio::time::sleep_until(deadline.into()).await,
                None => std::future::pending::<()>().await,
            }
        };

        tokio::select! {
            biased;

            // A queued snapshot is ready for delivery.
            () = deliver_tick => {
                for snapshot in queue.drain_ready(std::time::Instant::now()) {
                    if !forward_snapshot(snapshot_tx, snapshot) {
                        return;
                    }
                }
            }

            // A new datagram arrived from the server.
            result = connection.read_datagram() => {
                let bytes = match result {
                    Ok(bytes) => bytes,
                    Err(ConnectionError::ApplicationClosed(_) | ConnectionError::ConnectionClosed(_)) => {
                        info!("server closed connection");
                        break;
                    }
                    Err(e) => {
                        warn!(error = %e, "reading datagram failed");
                        continue;
                    }
                };

                match decode::<S>(&bytes) {
                    Ok(snapshot) => {
                        if delay.is_enabled() {
                            // Hold for the sampled delivery deadline.
                            queue.push(snapshot);
                        } else if !forward_snapshot(snapshot_tx, snapshot) {
                            break;
                        }
                    }
                    Err(e) => warn!(error = %e, "decoding snapshot failed"),
                }
            }
        }
    }
}

/// Forward one snapshot to the tick thread. Returns `false` if the
/// receiver is gone and the caller should stop.
fn forward_snapshot<S>(snapshot_tx: &crossbeam_channel::Sender<S>, snapshot: S) -> bool {
    match snapshot_tx.try_send(snapshot) {
        Ok(()) => true,
        Err(TrySendError::Full(_)) => {
            warn!("snapshot drain queue full; dropping snapshot");
            true
        }
        Err(TrySendError::Disconnected(_)) => {
            info!("snapshot receiver dropped; exiting");
            false
        }
    }
}

async fn send_requests<R>(mut send: SendStream, request_rx: &mut mpsc::Receiver<R>)
where
    R: Serialize,
{
    while let Some(request) = request_rx.recv().await {
        match encode_framed::<R>(&request) {
            Ok(bytes) => {
                if let Err(e) = send.write_all(&bytes).await {
                    error!(error = %e, "writing request to control stream");
                    break;
                }
            }
            Err(e) => warn!(error = %e, "encoding request failed"),
        }
    }
    send.finish().ok();
}

async fn recv_events<E>(mut recv: RecvStream, event_tx: &crossbeam_channel::Sender<E>)
where
    E: DeserializeOwned,
{
    let mut buf: Vec<u8> = Vec::with_capacity(4096);
    let mut chunk = [0_u8; 1024];

    loop {
        match recv.read(&mut chunk).await {
            Ok(Some(n)) => {
                buf.extend_from_slice(&chunk[..n]);
            }
            Ok(None) => {
                info!("control stream closed by server");
                break;
            }
            Err(e) => {
                warn!(error = %e, "reading control stream failed");
                break;
            }
        }

        loop {
            let (event, consumed) = match decode_framed::<E>(&mut buf) {
                Ok(Some((event, consumed))) => (event, consumed),
                Ok(None) => break,
                Err(e) => {
                    warn!(error = %e, "decoding event failed; dropping buffer");
                    buf.clear();
                    break;
                }
            };
            buf.drain(..consumed);

            match event_tx.try_send(event) {
                Ok(()) => {}
                Err(TrySendError::Full(_)) => {
                    warn!("event drain queue full; dropping event");
                }
                Err(TrySendError::Disconnected(_)) => {
                    debug!("event receiver dropped; exiting");
                    return;
                }
            }
        }
    }
}
