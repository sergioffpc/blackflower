# Platform-independent content roles

Status: accepted.

Packs describe content purpose through simulation and presentation roles. They
contain no deployment or platform profile. Simulation resources are shared by
simulation and prediction; presentation resources serve presentation. The server
consumes simulation content and the client consumes both roles. Runtime adapters
prepare platform-specific SDK resources from the portable resource contracts.

[Pack v1](../../schemas/pack/v1.md) defines the portable header and role values.
Both artifacts remain independently signed and share one scenario build
identity. Schema evolution follows the
[release policy](../development-process.md#schema-versioning).

This replaces the client/server pack split in
[ADR-0005](0005-require-signed-cooked-content.md) and the format and filename
selection in [ADR-0007](0007-minimal-pack-format-and-trust.md). Independent
trust, offline cooking and content validation remain required. Compatibility is
defined by resource schemas rather than a target-platform selector.
