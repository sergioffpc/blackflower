# blackflower-networking-quic

Low-level Quinn transport adapter for the Blackflower v1 application protocol.

This package owns QUIC endpoints, TLS configuration, stream roles, DATAGRAM I/O,
and bounded host queues. It is not a game client and does not own input,
prediction, rendering, bots, or the future shared client harness.

Unvalidated Initial packets consume only a constant-size global token bucket
before receiving Quinn's stateless Retry. Exhausted requests are ignored
without a response. Per-IP pending-handshake state is created only after the
Retry token has validated the remote address and is removed when the handshake
finishes.
