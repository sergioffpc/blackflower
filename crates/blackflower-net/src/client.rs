//! Client-side QUIC endpoint.
//!
//! Synchronous public API; internally drives a single-threaded tokio runtime.

use std::{net::SocketAddr, sync::Arc};

use anyhow::Context;
use quinn::{ClientConfig, Endpoint};
use tracing::info;

use crate::{
    cert::SkipServerVerification,
    messages::{ClientToServer, ServerToClient, decode, encode},
};

pub fn connect(server_addr: SocketAddr) -> anyhow::Result<()> {
    rustls::crypto::ring::default_provider()
        .install_default()
        .ok();

    let runtime = tokio::runtime::Builder::new_current_thread()
        .enable_all()
        .build()
        .context("building async runtime")?;

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

        let (mut send, mut recv) = connection
            .open_bi()
            .await
            .context("opening bidirectional stream")?;

        let msg = encode(&ClientToServer::Hello).context("encoding HELLO")?;
        send.write_all(&msg).await.context("sending HELLO")?;
        send.finish().context("finishing stream")?;
        info!("HELLO sent");

        let response = recv
            .read_to_end(1024)
            .await
            .context("reading ack payload")?;
        let msg: ServerToClient = decode(&response).context("decoding ack")?;
        info!(?msg, "received from server");

        connection.close(0_u32.into(), b"client done");
        endpoint.wait_idle().await;

        Ok(())
    })
}
