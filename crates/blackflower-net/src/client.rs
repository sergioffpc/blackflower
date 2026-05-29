//! Client-side QUIC endpoint.
//!
//! Synchronous public API; internally drives a single-threaded tokio runtime.

use std::{net::SocketAddr, sync::Arc, thread::JoinHandle};

use anyhow::Context;
use blackflower_core::ecs::Snapshot;
use quinn::{ClientConfig, Connection, ConnectionError, Endpoint};
use tokio::sync::oneshot;
use tracing::{error, info, warn};

use crate::{
    cert::SkipServerVerification,
    messages::{ClientToServer, ServerToClient, decode, encode},
};

pub struct ClientHandle {
    snapshot_rx: crossbeam_channel::Receiver<Snapshot>,
    shutdown_tx: Option<oneshot::Sender<()>>,
    join_handle: Option<JoinHandle<()>>,
}

impl ClientHandle {
    pub fn drain_snapshots(&self) -> Box<[Snapshot]> {
        let mut snapshots = vec![];
        while let Ok(snapshot) = self.snapshot_rx.try_recv() {
            snapshots.push(snapshot);
        }
        snapshots.into_boxed_slice()
    }
}

impl Drop for ClientHandle {
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

pub fn connect(server_addr: SocketAddr) -> anyhow::Result<ClientHandle> {
    rustls::crypto::ring::default_provider()
        .install_default()
        .ok();

    let (snapshot_tx, snapshot_rx) = crossbeam_channel::bounded::<Snapshot>(64);
    let (shutdown_tx, shutdown_rx) = oneshot::channel::<()>();

    let join_handle = std::thread::Builder::new()
        .name("blackflower-net::client".to_owned())
        .spawn(move || {
            if let Err(e) = start(server_addr, snapshot_tx, shutdown_rx) {
                error!(error = %e, "subscribe snapshots failed");
            }
        })
        .context("spawning network client thread")?;

    Ok(ClientHandle {
        snapshot_rx,
        shutdown_tx: Some(shutdown_tx),
        join_handle: Some(join_handle),
    })
}

fn start(
    server_addr: SocketAddr,
    snapshot_tx: crossbeam_channel::Sender<Snapshot>,
    shutdown_rx: oneshot::Receiver<()>,
) -> anyhow::Result<()> {
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

        let msg = encode(&ClientToServer::Subscribe).context("encoding subscription message")?;
        send.write_all(&msg)
            .await
            .context("sending subscription message")?;
        send.finish().context("finishing subscription stream")?;
        info!("subscribed for snapshots");

        tokio::select! {
            biased;

            _ = shutdown_rx => {
                info!("client received shutdown signal");
            }

            () = recv_loop(&connection, &snapshot_tx) => {}
        }

        connection.close(0_u32.into(), b"client done");
        endpoint.wait_idle().await;

        Ok(())
    })
}

async fn recv_loop(connection: &Connection, snapshot_tx: &crossbeam_channel::Sender<Snapshot>) {
    loop {
        let bytes = match connection.read_datagram().await {
            Ok(bytes) => bytes,
            Err(ConnectionError::ApplicationClosed(_) | ConnectionError::ConnectionClosed(_)) => {
                info!("server closed connection");
                break;
            }
            Err(e) => {
                warn!(error = %e, "reading datagram failed");
                break;
            }
        };

        match decode::<ServerToClient>(&bytes) {
            Ok(ServerToClient::Snapshot(snapshot)) => {
                if snapshot_tx.try_send(snapshot).is_err() {
                    info!("snapshot receiver dropped; exiting receive loop");
                    break;
                }
            }
            Err(e) => {
                warn!(error = %e, "decoding datagram failed");
            }
        }
    }
}
