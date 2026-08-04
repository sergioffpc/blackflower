use std::error::Error as StdError;
use std::net::{IpAddr, Ipv4Addr, SocketAddr, UdpSocket};
use std::num::{NonZeroU32, NonZeroUsize};
use std::time::Duration;

use blackflower_networking::{
    BootstrapId, ConnectionEpoch, DatagramHeader, FlowId, FlowSequence, ProjectionDigest,
    ProtocolRevision, SessionControlMessage, SimulationTick, StateBootstrapHeader,
    decode_control_message, encode_control_message, encode_datagram,
};
use blackflower_networking_quic::{
    AdmissionLimits, BootstrapTransfer, ClientConnection, ClientEndpointConfig, ClientTrustRoots,
    NetworkEvent, QuicClient, QuicServer, ServerConnection, ServerEndpointConfig, ServerTlsConfig,
};
use rcgen::{BasicConstraints, CertificateParams, CertifiedIssuer, IsCa, KeyPair};
use rustls::pki_types::{CertificateDer, PrivateKeyDer, PrivatePkcs8KeyDer};

type TestResult = Result<(), Box<dyn StdError>>;

#[tokio::test(flavor = "multi_thread", worker_threads = 2)]
async fn retry_control_datagram_and_bootstrap_follow_v1_roles() -> TestResult {
    let fixture = service_fixture("blackflower.test")?;
    let current_root = fixture.root.clone();
    let mut server = QuicServer::bind(server_config(fixture)?)?;
    let server_address = server.local_addr()?;
    let server_task = tokio::spawn(async move {
        let connection = server.accept().await?;
        Ok::<_, blackflower_networking_quic::QuicError>((server, connection))
    });
    let client = QuicClient::bind(client_config(
        server_address,
        "blackflower.test",
        current_root,
        None,
    ))?;
    let client_connection = client.connect().await?;
    let (server, server_connection) = server_task.await??;
    assert!(server.retries_sent() >= 1);

    assert_session_control(&client_connection, &server_connection).await?;

    let datagram = encode_datagram(
        DatagramHeader {
            flow: FlowId::TimeSync,
            connection_epoch: ConnectionEpoch::new(1),
            flow_sequence: FlowSequence::new(1),
        },
        &[1, 2, 3],
    );
    client_connection.send_datagram(datagram.clone())?;
    assert_eq!(server_connection.read_datagram().await?, datagram);

    assert_bootstrap(&client_connection, &server_connection).await?;

    assert!(client_connection.open_session_control().await.is_err());
    client.close(0, b"test complete");
    server.close(0, b"test complete");
    Ok(())
}

async fn assert_session_control(
    client: &ClientConnection,
    server: &ServerConnection,
) -> TestResult {
    let (client_control, server_control) = tokio::join!(
        client.open_session_control(),
        server.accept_session_control()
    );
    let mut client_control = client_control?;
    let mut server_control = server_control?;
    let request = SessionControlMessage::AdmissionRequest {
        ticket: b"one-use-ticket".to_vec(),
    };
    client_control
        .send(&encode_control_message(&request)?)
        .await?;
    assert_eq!(
        decode_control_message(&server_control.receive().await?)?,
        request
    );
    Ok(())
}

async fn assert_bootstrap(client: &ClientConnection, server: &ServerConnection) -> TestResult {
    let body = vec![0x5a; 1_024];
    let header = StateBootstrapHeader {
        bootstrap_id: BootstrapId::new(9),
        protocol_revision: ProtocolRevision::V1,
        snapshot_tick: SimulationTick::new(240),
        projection_digest: ProjectionDigest::from_bytes([7; 32]),
        body_length: u32::try_from(body.len())?,
    };
    let (sent, received) = tokio::join!(
        server.send_bootstrap(header, &body),
        client.receive_bootstrap()
    );
    sent?;
    assert_eq!(received?, BootstrapTransfer { header, body });
    Ok(())
}

#[tokio::test(flavor = "multi_thread", worker_threads = 2)]
async fn current_and_next_service_roots_overlap_without_mtls_or_zero_rtt() -> TestResult {
    let current = service_fixture("unused.test")?;
    let next = service_fixture("blackflower.test")?;
    let current_root = current.root;
    let next_root = next.root.clone();
    let mut server = QuicServer::bind(server_config(next)?)?;
    let address = server.local_addr()?;
    let server_task = tokio::spawn(async move { server.accept().await });
    let client = QuicClient::bind(client_config(
        address,
        "blackflower.test",
        current_root,
        Some(next_root),
    ))?;
    let client_connection = client.connect().await?;
    let server_connection = server_task.await??;
    assert!(client_connection.rtt() < Duration::from_secs(1));
    client_connection.close(0, b"test complete");
    server_connection.close(0, b"test complete");
    Ok(())
}

#[tokio::test(flavor = "multi_thread", worker_threads = 2)]
async fn nat_rebinding_emits_a_validated_path_change_and_preserves_datagrams() -> TestResult {
    let fixture = service_fixture("blackflower.test")?;
    let current_root = fixture.root.clone();
    let mut server = QuicServer::bind(server_config(fixture)?)?;
    let address = server.local_addr()?;
    let server_task = tokio::spawn(async move { server.accept().await });
    let client = QuicClient::bind(client_config(
        address,
        "blackflower.test",
        current_root,
        None,
    ))?;
    let client_connection = client.connect().await?;
    let sender = client_connection.clone();
    let server_connection = server_task.await??;
    let (client_handle, server_handle) =
        tokio::join!(client_connection.spawn_io(), server_connection.spawn_io());
    let client_handle = client_handle?;
    let server_handle = server_handle?;

    let old_port = client.local_addr()?.port();
    let rebound = UdpSocket::bind(SocketAddr::new(IpAddr::V4(Ipv4Addr::LOCALHOST), 0))?;
    let new_port = rebound.local_addr()?.port();
    assert_ne!(old_port, new_port);
    client.rebind(rebound)?;
    for sequence in 2_u32..=20 {
        sender.send_datagram(encode_datagram(
            DatagramHeader {
                flow: FlowId::TimeSync,
                connection_epoch: ConnectionEpoch::new(1),
                flow_sequence: FlowSequence::new(sequence),
            },
            &[4, 5, 6],
        ))?;
        tokio::time::sleep(Duration::from_millis(20)).await;
    }

    let changed = tokio::time::timeout(Duration::from_secs(2), async {
        loop {
            if let Some(NetworkEvent::PathChanged { previous, current }) =
                server_handle.try_receive()?
            {
                return Ok::<_, blackflower_networking_quic::QuicError>((previous, current));
            }
            tokio::time::sleep(Duration::from_millis(5)).await;
        }
    })
    .await??;
    assert_eq!(changed.0.port(), old_port);
    assert_eq!(changed.1.port(), new_port);
    drop(client_handle);
    drop(server_handle);
    Ok(())
}

#[tokio::test(flavor = "multi_thread", worker_threads = 2)]
async fn an_untrusted_service_ca_is_rejected() -> TestResult {
    let trusted = service_fixture("unused.test")?;
    let untrusted = service_fixture("blackflower.test")?;
    let mut server = QuicServer::bind(server_config(untrusted)?)?;
    let address = server.local_addr()?;
    let server_task = tokio::spawn(async move { server.accept().await });
    let client = QuicClient::bind(client_config(
        address,
        "blackflower.test",
        trusted.root,
        None,
    ))?;
    assert!(client.connect().await.is_err());
    server_task.abort();
    Ok(())
}

struct ServiceFixture {
    root: CertificateDer<'static>,
    chain: Vec<CertificateDer<'static>>,
    key: PrivateKeyDer<'static>,
}

fn service_fixture(server_name: &str) -> Result<ServiceFixture, Box<dyn StdError>> {
    let mut ca_params = CertificateParams::new(Vec::<String>::new())?;
    ca_params.is_ca = IsCa::Ca(BasicConstraints::Unconstrained);
    let ca_key = KeyPair::generate()?;
    let ca = CertifiedIssuer::self_signed(ca_params, ca_key)?;

    let leaf_params = CertificateParams::new(vec![server_name.to_owned()])?;
    let leaf_key = KeyPair::generate()?;
    let leaf = leaf_params.signed_by(&leaf_key, &ca)?;
    let key = PrivateKeyDer::Pkcs8(PrivatePkcs8KeyDer::from(leaf_key.serialize_der()));
    Ok(ServiceFixture {
        root: ca.der().clone(),
        chain: vec![leaf.der().clone(), ca.der().clone()],
        key,
    })
}

fn server_config(fixture: ServiceFixture) -> Result<ServerEndpointConfig, Box<dyn StdError>> {
    Ok(ServerEndpointConfig {
        bind_address: SocketAddr::new(IpAddr::V4(Ipv4Addr::LOCALHOST), 0),
        tls: ServerTlsConfig {
            certificate_chain: fixture.chain,
            private_key: fixture.key,
        },
        admission_limits: AdmissionLimits {
            attempts_per_window: NonZeroU32::new(32).ok_or("invalid attempt limit")?,
            window: Duration::from_secs(1),
            pending_per_origin: NonZeroUsize::new(4).ok_or("invalid pending limit")?,
            pending_global: NonZeroUsize::new(16).ok_or("invalid global pending limit")?,
            connections_global: NonZeroUsize::new(16).ok_or("invalid connection limit")?,
        },
    })
}

fn client_config(
    server_address: SocketAddr,
    server_name: &str,
    current: CertificateDer<'static>,
    next: Option<CertificateDer<'static>>,
) -> ClientEndpointConfig {
    ClientEndpointConfig {
        bind_address: SocketAddr::new(IpAddr::V4(Ipv4Addr::UNSPECIFIED), 0),
        server_address,
        server_name: server_name.to_owned(),
        trust_roots: ClientTrustRoots { current, next },
    }
}
