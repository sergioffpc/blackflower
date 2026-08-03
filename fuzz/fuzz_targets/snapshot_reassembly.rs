#![no_main]

use std::time::Duration;

use blackflower_networking::decode_snapshot_chunk;
use blackflower_networking_replication::SnapshotReassembler;
use libfuzzer_sys::fuzz_target;

fuzz_target!(|bytes: &[u8]| {
    let mut cursor = bytes;
    let mut reassembler: Option<SnapshotReassembler> = None;
    let mut elapsed_micros = 0_u64;
    while cursor.len() >= 2 {
        let length = usize::from(u16::from_le_bytes([cursor[0], cursor[1]]));
        cursor = &cursor[2..];
        if length > cursor.len() {
            break;
        }
        let candidate = &cursor[..length];
        cursor = &cursor[length..];
        let Ok(chunk) = decode_snapshot_chunk(candidate, 1_000) else {
            continue;
        };
        let now = Duration::from_micros(elapsed_micros);
        elapsed_micros = elapsed_micros.saturating_add(10_000);
        match &mut reassembler {
            Some(active) => {
                let _result = active.push(chunk, now);
            }
            None => {
                reassembler = SnapshotReassembler::new(chunk, now).ok();
            }
        }
    }
});
