#![no_main]

use blackflower_networking::{
    decode_control_message, decode_datagram, decode_input_datagram, decode_snapshot_applied_ack,
    decode_snapshot_chunk, decode_state_bootstrap_header, decode_time_sync,
};
use libfuzzer_sys::fuzz_target;

fuzz_target!(|bytes: &[u8]| {
    let _datagram = decode_datagram(bytes);
    let _control = decode_control_message(bytes);
    let _input = decode_input_datagram(bytes);
    let _time = decode_time_sync(bytes);
    let _ack = decode_snapshot_applied_ack(bytes);
    let _chunk = decode_snapshot_chunk(bytes, 1_000);
    let _bootstrap = decode_state_bootstrap_header(bytes);
});
