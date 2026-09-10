# Independent pack fixtures

The `.bfserver`, `.bfagent`, `.bfclient`, and binary `.pub` files are stored
with Git LFS. Follow the
[Git setup](../../../docs/git-workflow.md#starting-work) and run `git lfs pull`
before testing a clone that contains LFS pointers.

The reference pack encodes literal fields from
[pack v1](../../../schemas/pack/v1.md) and
[scene v1](../../../schemas/scene/v1.md), independently of the cooker. Its
signature uses a disposable Ed25519 key; only the public key and signed
artifacts are retained.

`reference.json` fixes the scene bytes and expected artifact/build identities.
The source/settings digests and provenance identify reference data rather than
an actual source cook. Python and C++ consume the same fixtures using separate
cryptographic implementations.

The #61 fixtures use local collider records and a prototype translated by seven
metres on X. The fixed local centres remain -3 and 1 metres on X; their world
centres would be 4 and 8 metres. The payload bytes, SceneAsset/ColliderAsset
IDs, new disposable public key and signatures were regenerated together,
retaining schema version 1. Asset IDs follow the exact domain-separated schema
transcript.
