use std::error::Error as StdError;
use std::io;
use std::net::{IpAddr, Ipv4Addr, SocketAddr};
use std::num::{NonZeroU32, NonZeroUsize};
use std::str::FromStr as _;
use std::sync::Arc;
use std::sync::atomic::{AtomicBool, Ordering};
use std::time::{Duration, Instant};

use blackflower_harness::{ClientHarness, ClientHarnessConfig, ClientPrediction, PredictionUpdate};
use blackflower_networking::{
    AdmissionClaims, AuthorityError, BudgetTier, CompatibilityContract, ConnectionEpoch,
    ContentManifest, IssuedResumeToken, MapId, MatchId, PlayerId, ProtocolRevision,
    RequiredContentSetId, ResumeClaims, SessionAuthority, SessionControlMessage, SessionId,
    SessionState,
};
use blackflower_networking_quic::{
    AdmissionLimits, ClientEndpointConfig, ClientNetworkHandle, ClientTrustRoot, NetworkEvent,
    QuicClient, QuicServer, ServerEndpointConfig, ServerTlsConfig,
};
use blackflower_networking_replication::{Snapshot, SnapshotTick};
use blackflower_server::{
    DedicatedServerNetwork, LoopbackSessionAuthority, ServerNetworkRuntime, SimulationHost,
};
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
    let content = content()?;
    let mut server = DedicatedServerNetwork::new(
        endpoint,
        TestAuthority::new(contract),
        contract,
        content.clone(),
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

    server.admit(&mut peer, ProtocolRevision::V1, Duration::ZERO)?;
    assert_admission_messages(&client, &content).await?;
    server.content_ready(&mut peer, &content)?;
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

#[tokio::test(flavor = "multi_thread", worker_threads = 2)]
async fn real_harness_reaches_active_through_the_server_supervisor() -> TestResult {
    let contract = contract();
    let content = content()?;
    let fixture = service_fixture("blackflower.test")?;
    let root = fixture.root.clone();
    let endpoint = QuicServer::bind(server_config(fixture)?)?;
    let address = endpoint.local_addr()?;
    let authority = LoopbackSessionAuthority::new(contract);
    let simulation = SimulationHost::spawn()?;
    let server = DedicatedServerNetwork::new(
        endpoint,
        authority,
        contract,
        content.clone(),
        BudgetTier::Constrained,
        Duration::ZERO,
    );
    let stop = Arc::new(AtomicBool::new(false));
    let runtime = ServerNetworkRuntime::new(server, simulation.status());
    let server_stop = Arc::clone(&stop);
    let server_task = tokio::spawn(async move { runtime.run(server_stop).await });

    let client_endpoint = QuicClient::bind(client_config(address, root))?;
    let connection = client_endpoint.connect().await?;
    let transport = connection.spawn_io().await?;
    let mut harness = ClientHarness::new(
        transport,
        EmptyPrediction::default(),
        ClientHarnessConfig {
            compatibility: contract,
            installed_content_set_id: content.required_content_set_id,
        },
    )?;
    let started = Instant::now();
    tokio::time::timeout(Duration::from_secs(4), async {
        loop {
            harness.update(
                started.elapsed(),
                blackflower_networking::SimulationTick::new(simulation.completed_ticks()),
            )?;
            if harness.view().session_state() == SessionState::Active {
                return Ok::<_, Box<dyn StdError>>(());
            }
            tokio::time::sleep(Duration::from_millis(5)).await;
        }
    })
    .await??;

    stop.store(true, Ordering::Release);
    tokio::time::timeout(Duration::from_secs(1), server_task).await???;
    let _exit = simulation.shutdown()?;
    Ok(())
}

#[derive(Debug, Default)]
struct EmptyPrediction {
    tick: blackflower_networking::SimulationTick,
    state: Option<blackflower_networking::SimulationTick>,
}

impl ClientPrediction for EmptyPrediction {
    type State = blackflower_networking::SimulationTick;
    type Error = io::Error;

    fn current_tick(&self) -> blackflower_networking::SimulationTick {
        self.tick
    }

    fn bootstrap(&mut self, snapshot: &Snapshot) -> Result<PredictionUpdate, Self::Error> {
        self.install_empty_snapshot(snapshot, true)
    }

    fn apply_snapshot(&mut self, snapshot: &Snapshot) -> Result<PredictionUpdate, Self::Error> {
        self.install_empty_snapshot(snapshot, false)
    }

    fn queue_control(
        &mut self,
        _frame: &blackflower_networking::ControlFrame,
    ) -> Result<(), Self::Error> {
        Err(io::Error::other(
            "empty prediction does not accept gameplay controls",
        ))
    }

    fn advance_to(
        &mut self,
        target: blackflower_networking::SimulationTick,
    ) -> Result<(), Self::Error> {
        self.tick = target;
        self.state = Some(target);
        Ok(())
    }

    fn predicted_state(&self) -> Option<&Self::State> {
        self.state.as_ref()
    }
}

impl EmptyPrediction {
    fn install_empty_snapshot(
        &mut self,
        snapshot: &Snapshot,
        bootstrap: bool,
    ) -> Result<PredictionUpdate, io::Error> {
        if !snapshot.is_empty() {
            return Err(io::Error::other("expected an empty bootstrap snapshot"));
        }
        self.tick = blackflower_networking::SimulationTick::new(snapshot.tick().get());
        self.state = Some(self.tick);
        if bootstrap {
            Ok(PredictionUpdate::Bootstrapped { tick: self.tick })
        } else {
            Ok(PredictionUpdate::Converged { tick: self.tick })
        }
    }
}

async fn assert_admission_messages(
    client: &ClientNetworkHandle,
    content: &ContentManifest,
) -> TestResult {
    assert!(matches!(
        next_control(client).await?,
        SessionControlMessage::AdmissionAccepted {
            connection_epoch,
            ..
        } if connection_epoch == ConnectionEpoch::new(1)
    ));
    assert!(matches!(
        next_control(client).await?,
        SessionControlMessage::ResumeIssued { .. }
    ));
    assert_eq!(
        next_control(client).await?,
        SessionControlMessage::ContentManifest(content.clone())
    );
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
    resume_available: bool,
}

impl TestAuthority {
    fn new(contract: CompatibilityContract) -> Self {
        Self {
            claims: claims(contract),
            resume_available: true,
        }
    }
}

impl SessionAuthority for TestAuthority {
    fn admit(&mut self, _now: Duration) -> Result<AdmissionClaims, AuthorityError> {
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
    }
}

fn content() -> TestResult<ContentManifest> {
    Ok(ContentManifest {
        map_id: MapId::from_str("maps/test")?,
        required_content_set_id: RequiredContentSetId::from_bytes([2; 32]),
    })
}

fn claims(contract: CompatibilityContract) -> AdmissionClaims {
    AdmissionClaims {
        session_id: SessionId::from_bytes([3; 16]),
        player_id: PlayerId::from_bytes([4; 16]),
        match_id: MatchId::from_bytes([5; 16]),
        protocol_revision: contract.protocol_revision,
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
            pending_global: NonZeroUsize::new(16).ok_or("invalid global pending")?,
            connections_global: NonZeroUsize::new(16).ok_or("invalid connections")?,
        },
    })
}

fn client_config(address: SocketAddr, root: CertificateDer<'static>) -> ClientEndpointConfig {
    ClientEndpointConfig {
        bind_address: SocketAddr::new(IpAddr::V4(Ipv4Addr::UNSPECIFIED), 0),
        server_address: address,
        server_name: "blackflower.test".to_owned(),
        trust_root: ClientTrustRoot { current: root },
    }
}
