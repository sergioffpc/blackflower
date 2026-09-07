# Require verified signed packs of cooked content

Status: accepted for the requirement, explicitly requested by the owner. Pack format, cryptographic algorithms, and tool integration below remain proposals.

The MVP consumes content prepared offline by a cooker and distributed in a signed pack. Server and clients validate the pack before creating their worlds or admitting gameplay; they do not cook source assets during startup or simulation. Pack I/O, signature/integrity checks, decoding, and SDK resource creation execute outside ECS, preserving [ADR-0004](0004-keep-io-outside-ecs.md).

This makes the cooker and pack loader part of the first delivery instead of deferring the content pipeline. It introduces artifact compatibility and signing-key management, while providing an explicit content identity and rejecting altered or unauthenticated inputs. Loading loose source assets or silently bypassing failed verification would violate this requirement. The [cooker proposal](../cooker-and-packs.md) defines the intended artifact, trust model, and remaining feasibility work.
