use std::error::Error as StdError;
use std::net::{IpAddr, Ipv4Addr, SocketAddr};
use std::num::{NonZeroU32, NonZeroUsize};
use std::time::Duration;

use blackflower_networking::{
    AdmissionClaims, AuthorityError, BudgetTier, CompatibilityContract, ConnectionEpoch,
    IssuedResumeToken, MatchId, PlayerId, ProtocolRevision, RequiredContentSetId, ResumeClaims,
    SessionAuthority, SessionControlMessage, SessionId, SessionState, SimulationCompatibilityId,
};
use blackflower_networking_quic::{
    AdmissionLimits, ClientEndpointConfig, ClientNetworkHandle, ClientTrustRoots, NetworkEvent,
    QuicClient, QuicServer, ServerEndpointConfig, ServerTlsConfig,
};
use blackflower_networking_replication::{Snapshot, SnapshotTick};
use blackflower_server::DedicatedServerNetwork;
use rcgen::{BasicConstraints, CertificateParams, CertifiedIssuer, IsCa, KeyPair};
use rustls::pki_types::{CertificateDer, PrivateKeyDer, PrivatePkcs8KeyDer};

type TestResult<T = ()> = Result<T, Box<dyn StdError>>;

#[tokio::test(flavor = "multi_thread", worker_threads = 2)]
async fn dedicated_server_composes_admission_bootstrap_activation_and_resume() -> TestResult {
    let contract = contract();
    let fixture = service_fixture("blackflower.test")?;
    let root = fixture.root.clone();
    let endpoint = QuicServer::bind(server_config(fixture)?)?;
    let address = endpoint.local_addr()?;
    let mut server = DedicatedServerNetwork::new(
        endpoint,
        TestAuthority::new(contract),
        contract,
        BudgetTier::Constrained,
        Duration::ZERO,
    );
    let accepted = tokio::spawn(async move {
        let peer = server.accept(Duration::ZERO).await?;
        Ok::<_, blackflower_server::PeerError>((server, peer))
    });
    let client = QuicClient::bind(client_config(address, root))?;
    let connection = client.connect().await?;
    let (accepted, client) = tokio::join!(accepted, connection.spawn_io());
    let (mut server, mut peer) = accepted??;
    let client = client?;

    server.admit(&mut peer, b"ticket", Duration::ZERO)?;
    assert_admission_messages(&client).await?;
    assert_bootstrap_and_activation(&mut peer, &client).await?;
    let resumed = server.resume(&mut peer, b"resume", Duration::from_secs(1))?;
    assert_eq!(resumed.claims.connection_epoch, ConnectionEpoch::new(2));
    assert_eq!(peer.session().state(), SessionState::Synchronizing);
    assert!(
        server
            .resume(&mut peer, b"resume", Duration::from_secs(2))
            .is_err()
    );
    Ok(())
}

async fn assert_admission_messages(client: &ClientNetworkHandle) -> TestResult {
    assert!(matches!(
        next_control(client).await?,
        SessionControlMessage::AdmissionAccepted(_)
    ));
    assert!(matches!(
        next_control(client).await?,
        SessionControlMessage::ResumeIssued { .. }
    ));
    Ok(())
}

async fn assert_bootstrap_and_activation(
    peer: &mut blackflower_server::NetworkPeer,
    client: &ClientNetworkHandle,
) -> TestResult {
    let snapshot = Snapshot::new(SnapshotTick::new(0), [])?;
    let bootstrap_id = peer.queue_bootstrap(snapshot)?;
    let transfer = next_bootstrap(client).await?;
    assert_eq!(transfer.header.bootstrap_id, bootstrap_id);
    peer.bootstrap_applied(
        bootstrap_id,
        transfer.header.snapshot_tick,
        transfer.header.projection_digest,
    )?;
    let tick = peer.schedule_activation(transfer.header.snapshot_tick, 0)?;
    assert!(peer.advance(tick)?);
    assert_eq!(peer.session().state(), SessionState::Active);
    Ok(())
}

async fn next_control(client: &ClientNetworkHandle) -> TestResult<SessionControlMessage> {
    let event = next_event(client, |event| {
        matches!(event, NetworkEvent::SessionControl(_))
    })
    .await?;
    let NetworkEvent::SessionControl(frame) = event else {
        return Err("expected session control".into());
    };
    Ok(blackflower_networking::decode_control_message(&frame)?)
}

async fn next_bootstrap(
    client: &ClientNetworkHandle,
) -> TestResult<blackflower_networking_quic::BootstrapTransfer> {
    let event = next_event(client, |event| matches!(event, NetworkEvent::Bootstrap(_))).await?;
    let NetworkEvent::Bootstrap(transfer) = event else {
        return Err("expected bootstrap".into());
    };
    Ok(transfer)
}

async fn next_event(
    client: &ClientNetworkHandle,
    select: impl Fn(&NetworkEvent) -> bool,
) -> TestResult<NetworkEvent> {
    Ok(tokio::time::timeout(Duration::from_secs(2), async {
        loop {
            if let Some(event) = client.try_receive()?
                && select(&event)
            {
                return Ok::<_, blackflower_networking_quic::QuicError>(event);
            }
            tokio::time::sleep(Duration::from_millis(5)).await;
        }
    })
    .await??)
}

#[derive(Debug)]
struct TestAuthority {
    claims: AdmissionClaims,
    ticket_available: bool,
    resume_available: bool,
}

impl TestAuthority {
    fn new(contract: CompatibilityContract) -> Self {
        Self {
            claims: claims(contract),
            ticket_available: true,
            resume_available: true,
        }
    }
}

impl SessionAuthority for TestAuthority {
    fn consume_admission(
        &mut self,
        ticket: &[u8],
        now: Duration,
    ) -> Result<AdmissionClaims, AuthorityError> {
        if ticket != b"ticket" {
            return Err(AuthorityError::Invalid);
        }
        if now > Duration::from_secs(60) {
            return Err(AuthorityError::Expired);
        }
        if !std::mem::take(&mut self.ticket_available) {
            return Err(AuthorityError::Replayed);
        }
        Ok(self.claims)
    }

    fn issue_resume(
        &mut self,
        _claims: &AdmissionClaims,
        now: Duration,
    ) -> Result<IssuedResumeToken, AuthorityError> {
        Ok(IssuedResumeToken {
            token: b"resume".to_vec(),
            expires_at: now.saturating_add(Duration::from_secs(30)),
        })
    }

    fn consume_resume(
        &mut self,
        token: &[u8],
        now: Duration,
    ) -> Result<ResumeClaims, AuthorityError> {
        if token != b"resume" {
            return Err(AuthorityError::Invalid);
        }
        if now > Duration::from_secs(30) {
            return Err(AuthorityError::Expired);
        }
        if !std::mem::take(&mut self.resume_available) {
            return Err(AuthorityError::Replayed);
        }
        Ok(ResumeClaims {
            session_id: self.claims.session_id,
            player_id: self.claims.player_id,
            match_id: self.claims.match_id,
            connection_epoch: ConnectionEpoch::new(2),
        })
    }
}

fn contract() -> CompatibilityContract {
    CompatibilityContract {
        protocol_revision: ProtocolRevision::V1,
        simulation_compatibility_id: SimulationCompatibilityId::from_bytes([1; 32]),
        required_content_set_id: RequiredContentSetId::from_bytes([2; 32]),
    }
}

fn claims(contract: CompatibilityContract) -> AdmissionClaims {
    AdmissionClaims {
        session_id: SessionId::from_bytes([3; 16]),
        player_id: PlayerId::from_bytes([4; 16]),
        match_id: MatchId::from_bytes([5; 16]),
        protocol_revision: contract.protocol_revision,
        simulation_compatibility_id: contract.simulation_compatibility_id,
        required_content_set_id: contract.required_content_set_id,
    }
}

struct ServiceFixture {
    root: CertificateDer<'static>,
    chain: Vec<CertificateDer<'static>>,
    key: PrivateKeyDer<'static>,
}

fn service_fixture(server_name: &str) -> TestResult<ServiceFixture> {
    let mut ca_params = CertificateParams::new(Vec::<String>::new())?;
    ca_params.is_ca = IsCa::Ca(BasicConstraints::Unconstrained);
    let ca = CertifiedIssuer::self_signed(ca_params, KeyPair::generate()?)?;
    let leaf_key = KeyPair::generate()?;
    let leaf = CertificateParams::new(vec![server_name.to_owned()])?.signed_by(&leaf_key, &ca)?;
    Ok(ServiceFixture {
        root: ca.der().clone(),
        chain: vec![leaf.der().clone(), ca.der().clone()],
        key: PrivateKeyDer::Pkcs8(PrivatePkcs8KeyDer::from(leaf_key.serialize_der())),
    })
}

fn server_config(fixture: ServiceFixture) -> TestResult<ServerEndpointConfig> {
    Ok(ServerEndpointConfig {
        bind_address: SocketAddr::new(IpAddr::V4(Ipv4Addr::LOCALHOST), 0),
        tls: ServerTlsConfig {
            certificate_chain: fixture.chain,
            private_key: fixture.key,
        },
        admission_limits: AdmissionLimits {
            attempts_per_window: NonZeroU32::new(32).ok_or("invalid attempts")?,
            window: Duration::from_secs(1),
            pending_per_origin: NonZeroUsize::new(4).ok_or("invalid pending")?,
        },
    })
}

fn client_config(address: SocketAddr, root: CertificateDer<'static>) -> ClientEndpointConfig {
    ClientEndpointConfig {
        bind_address: SocketAddr::new(IpAddr::V4(Ipv4Addr::UNSPECIFIED), 0),
        server_address: address,
        server_name: "blackflower.test".to_owned(),
        trust_roots: ClientTrustRoots {
            current: root,
            next: None,
        },
    }
}
