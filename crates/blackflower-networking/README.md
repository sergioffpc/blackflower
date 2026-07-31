# Blackflower networking contracts

Stage 9 defines strict version-1 codecs for `VoiceCapturePacket`,
`AudibleSoundDelivery`, and `AudibleVoiceDelivery`, plus a bounded in-memory
client/server harness. Unknown versions, reserved bits, truncation, trailing
bytes, oversized payloads, duplicates, and packets outside the 60 ms reorder
window are rejected deterministically.

The voice capture payload never carries sender identity; the host session owns
that binding. Audible voice preserves the exact original bounded Opus packet
and is encoded only after authoritative audibility succeeds. No production
sockets, authentication, congestion control, or clock synchronization are
implemented in this crate.
