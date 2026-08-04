# Blackflower networking contracts

This crate defines the transport-independent Blackflower v1 protocol: explicit
wire codecs, session and clock machines, protocol/content negotiation, session
identity and resume authority boundaries, input deduplication and command
timing, bounded scheduling, voice routing, and network observability. Its voice
support owns only session routing, queueing, and opaque stream identifiers;
acoustic application payloads are not decoded.

The crate performs no socket I/O and does not reimplement QUIC cryptography,
transport ACKs, loss recovery, congestion control, pacing, or path validation.
Production transport lives in `blackflower-networking-quic`. The consolidated
requirements are in `docs/networking-v1.md`.
