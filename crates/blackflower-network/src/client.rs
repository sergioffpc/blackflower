//! Client-side QUIC endpoint.
//!
//! Synchronous public API; internally drives a single-threaded tokio runtime.

use std::{net::SocketAddr, sync::Arc, thread::JoinHandle};

use anyhow::Context;
use quinn::{ClientConfig, Connection, ConnectionError, Endpoint};
use serde::{Serialize, de::DeserializeOwned};
use tokio::sync::{mpsc, oneshot};
use tracing::{error, info, warn};

use crate::{
    cert::SkipServerVerification,
    messages::{Control, decode, encode},
};

/// Bounded capacity of the command queue from the caller to the network.
const INPUT_QUEUE_CAPACITY: usize = 16;

/// Bounded capacity of the tick → network snapshot queue.
const WORLD_QUEUE_CAPACITY: usize = 8;

pub struct ClientHandle<I, W> {
    input_tx: mpsc::Sender<I>,
    world_rx: crossbeam_channel::Receiver<W>,
    shutdown_tx: Option<oneshot::Sender<()>>,
    join_handle: Option<JoinHandle<()>>,
}

impl<I, W> ClientHandle<I, W> {
    pub fn try_recv_world_snapshots(&self) -> Box<[W]> {
        let mut snapshots = vec![];
        while let Ok(snapshot) = self.world_rx.try_recv() {
            snapshots.push(snapshot);
        }
        snapshots.into_boxed_slice()
    }

    pub fn try_send_input_snapshot(&self, input: I)
    where
        I: Send + Sync + 'static,
    {
        match self.input_tx.try_send(input) {
            Ok(()) => {}
            Err(mpsc::error::TrySendError::Full(_)) => {
                warn!("input queue full; dropping input");
            }
            Err(mpsc::error::TrySendError::Closed(_)) => {
                warn!("input channel closed");
            }
        }
    }
}

impl<I, W> Drop for ClientHandle<I, W> {
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

pub fn connect<I, W>(server_addr: SocketAddr) -> anyhow::Result<ClientHandle<I, W>>
where
    I: Serialize + Send + 'static,
    W: DeserializeOwned + Send + 'static,
{
    rustls::crypto::ring::default_provider()
        .install_default()
        .ok();

    let (input_tx, input_rx) = mpsc::channel::<I>(INPUT_QUEUE_CAPACITY);
    let (world_tx, world_rx) = crossbeam_channel::bounded::<W>(WORLD_QUEUE_CAPACITY);
    let (shutdown_tx, shutdown_rx) = oneshot::channel::<()>();

    let join_handle = std::thread::Builder::new()
        .name("blackflower-net::client".to_owned())
        .spawn(move || {
            if let Err(e) = start(server_addr, input_rx, world_tx, shutdown_rx) {
                error!(error = %e, "subscribe snapshots failed");
            }
        })
        .context("spawning network client thread")?;

    Ok(ClientHandle {
        input_tx,
        world_rx,
        shutdown_tx: Some(shutdown_tx),
        join_handle: Some(join_handle),
    })
}

fn start<I, W>(
    server_addr: SocketAddr,
    mut input_rx: mpsc::Receiver<I>,
    world_tx: crossbeam_channel::Sender<W>,
    shutdown_rx: oneshot::Receiver<()>,
) -> anyhow::Result<()>
where
    I: Serialize,
    W: DeserializeOwned,
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

        let (mut send, _recv) = connection
            .open_bi()
            .await
            .context("opening bidirectional stream")?;

        let msg = encode(&Control::Connect).context("encoding subscription message")?;
        send.write_all(&msg)
            .await
            .context("sending connection message")?;
        send.finish().context("finishing connection stream")?;

        tokio::select! {
            biased;

            _ = shutdown_rx => {
                info!("client received shutdown signal");
            }
            () = send_input_snapshots(&connection, &mut input_rx) => {}
            () = recv_world_snapshots(&connection, &world_tx) => {}
        }

        connection.close(0_u32.into(), b"client done");
        endpoint.wait_idle().await;

        Ok(())
    })
}

async fn send_input_snapshots<I>(connection: &Connection, input_rx: &mut mpsc::Receiver<I>)
where
    I: Serialize,
{
    while let Some(input) = input_rx.recv().await {
        match connection.open_uni().await {
            Ok(mut send) => match encode::<I>(&input) {
                Ok(bytes) => {
                    if let Err(e) = send.write_all(&bytes).await {
                        warn!(error = %e, "sending input stream");
                        continue;
                    }
                    if let Err(e) = send.finish() {
                        warn!(error = %e, "finishing input stream");
                    }
                }
                Err(e) => warn!(error = %e, "encoding input failed"),
            },
            Err(e) => {
                warn!(error = %e, "opening input stream");
                break;
            }
        }
    }
}

async fn recv_world_snapshots<W>(connection: &Connection, world_tx: &crossbeam_channel::Sender<W>)
where
    W: DeserializeOwned,
{
    loop {
        let bytes = match connection.read_datagram().await {
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

        match decode::<W>(&bytes) {
            Ok(world) => {
                if let Err(e) = world_tx.try_send(world) {
                    error!(error = %e,"snapshot receiver dropped; exiting receive loop");
                    break;
                }
            }
            Err(e) => {
                warn!(error = %e, "decoding datagram failed");
            }
        }
    }
}
