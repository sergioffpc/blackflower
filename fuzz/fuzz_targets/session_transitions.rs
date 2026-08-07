#![no_main]

use std::time::Duration;

use blackflower_networking::{
    AdmissionClaims, ClientSession, CompatibilityContract, ConnectionEpoch, MatchId, PlayerId,
    ProtocolRevision, SessionId, SimulationTick,
};
use libfuzzer_sys::fuzz_target;

fuzz_target!(|operations: &[u8]| {
    let contract = CompatibilityContract {
        protocol_revision: ProtocolRevision::V1,
    };
    let claims = AdmissionClaims {
        session_id: SessionId::from_bytes([3; 16]),
        player_id: PlayerId::from_bytes([4; 16]),
        match_id: MatchId::from_bytes([5; 16]),
        protocol_revision: ProtocolRevision::V1,
    };
    let mut session = ClientSession::new(contract, ConnectionEpoch::new(1));
    for (index, operation) in operations.iter().copied().enumerate() {
        let tick = SimulationTick::new(u64::try_from(index).unwrap_or(u64::MAX));
        match operation % 9 {
            0 => {
                let _result = session.secure();
            }
            1 => {
                let _result = session.negotiate();
            }
            2 => {
                let _result = session.accept_claims(&claims);
            }
            3 => {
                let _result = session.synchronize();
            }
            4 => {
                let _result = session.schedule_activation(tick, SimulationTick::new(24));
            }
            5 => {
                let _result = session.advance(tick);
            }
            6 => {
                let _result = session.begin_resync(Duration::from_millis(
                    u64::try_from(index).unwrap_or(u64::MAX),
                ));
            }
            7 => {
                let _result = session.reconnect(ConnectionEpoch::new(u32::from(operation) + 2));
            }
            8 => {
                let _result = session.close();
            }
            _ => {}
        }
    }
});
