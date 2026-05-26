//! Server-side QUIC endpoint.
//!
//! Spawns a tokio runtime on a dedicated background thread, binds a QUIC
//! endpoint, and accepts incoming connections.
//!
//! The public API ([`ServerHandle`]) is synchronous and intended to be used
//! from the tick loop. The handle owns the background thread; dropping it
//! signals shutdown.

use std::{net::SocketAddr, sync::Arc, thread::JoinHandle};

use anyhow::Context;
use quinn::{Endpoint, Incoming, ServerConfig};
use tokio::sync::oneshot;
use tracing::{error, info};

use crate::{
    cert::generate_self_signed_cert,
    messages::{ClientToServer, ServerToClient, decode, encode},
};

/// Handle to a running server endpoint.
///
/// The server runs on a dedicated background thread that owns a tokio runtime.
/// When this handle is dropped, the thread is signaled to shut down and joined.
pub struct ServerHandle {
    shutdown_tx: Option<oneshot::Sender<()>>,
    join_handle: Option<JoinHandle<()>>,
}

impl Drop for ServerHandle {
    fn drop(&mut self) {
        if let Some(tx) = self.shutdown_tx.take() {
            tx.send(()).ok();
        }
        if let Some(handle) = self.join_handle.take()
            && let Err(err) = handle.join()
        {
            error!(error = ?err, "network thread");
        }
    }
}

pub fn start(bind_addr: SocketAddr) -> anyhow::Result<ServerHandle> {
    rustls::crypto::ring::default_provider()
        .install_default()
        .ok();

    let (certs, key) = generate_self_signed_cert().context("self-signed cert")?;
    let mut rustls_config = rustls::ServerConfig::builder()
        .with_no_client_auth()
        .with_single_cert(certs, key)
        .context("rustls server config")?;
    rustls_config.alpn_protocols = vec![b"blackflower/0".to_vec()];

    let quic_config = quinn::crypto::rustls::QuicServerConfig::try_from(rustls_config)
        .context("converting rustls to QUIC server config")?;
    let server_config = ServerConfig::with_crypto(Arc::new(quic_config));

    let (shutdown_tx, shutdown_rx) = oneshot::channel();

    let join_handle = std::thread::Builder::new()
        .name("blackflower-net::server".to_owned())
        .spawn(move || {
            let runtime = match tokio::runtime::Builder::new_current_thread()
                .enable_all()
                .build()
            {
                Ok(rt) => rt,
                Err(e) => {
                    error!(error = %e, "building async runtime");
                    return;
                }
            };

            runtime.block_on(async move {
                match Endpoint::server(server_config, bind_addr) {
                    Ok(ep) => {
                        let local_addr = ep.local_addr().unwrap_or(bind_addr);
                        info!(local = %local_addr, "listening");

                        accept_loop(ep, shutdown_rx).await;
                    }
                    Err(e) => {
                        error!(error = %e, "server endpoint");
                    }
                }
            });
        })
        .context("spawning network thread")?;

    Ok(ServerHandle {
        shutdown_tx: Some(shutdown_tx),
        join_handle: Some(join_handle),
    })
}

async fn accept_loop(endpoint: Endpoint, mut shutdown_rx: oneshot::Receiver<()>) {
    loop {
        tokio::select! {
            // Prioritize shutdown handling over incoming connections.
            // With `biased`, Tokio evaluates branches top-to-bottom, so if both
            // shutdown and accept are ready at the same time, shutdown wins.
            // This avoids accepting new connections while shutting down.
            biased;

            _ = &mut shutdown_rx => {
                info!("shutdown signal received; closing endpoint");
                endpoint.close(0_u32.into(), b"server shutted down");
                break;
            }

            incoming = endpoint.accept() => {
                let Some(incoming) = incoming else {
                    info!("endpoint closed; exiting accept loop");
                    break;
                };
                tokio::spawn(async move {
                    if let Err(e) = handle_connection(incoming).await {
                        error!(error = %e, "connection closed");
                    }
                });
            }
        }
    }

    endpoint.wait_idle().await;
}

async fn handle_connection(incoming: Incoming) -> anyhow::Result<()> {
    let connection = incoming.await.context("connection handshake failed")?;
    let remote_addr = connection.remote_address();
    info!(remote = %remote_addr, "client connected");

    let (mut send, mut recv) = connection
        .accept_bi()
        .await
        .context("accepting bidirectional stream")?;

    let buf = recv.read_to_end(1024).await.context("reading HELLO")?;
    let msg: ClientToServer = decode(&buf).context("decoding HELLO")?;
    info!(remote = %remote_addr, message = ?msg, "received from client");

    let reply = encode(&ServerToClient::Ack).context("encoding ACK")?;
    send.write_all(&reply).await.context("sending ACK")?;
    send.finish().context("finishing stream")?;
    info!(remote = %remote_addr, "ACK sent");

    // Wait for the client to close the connection cleanly.
    connection.closed().await;
    info!(remote = %remote_addr, "client disconnected");
    Ok(())
}
