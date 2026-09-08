# Independent pack fixtures

The `.bfclient`, `.bfserver`, and binary `.pub` files are stored with Git LFS.
Follow the [Git setup](../../../docs/git-workflow.md#starting-work) and run
`git lfs pull` before testing an existing clone that contains LFS pointers.

The two binary reference packs were encoded from literal fields in the pack-v1
and scene-v1 specifications by a standalone script that did not import the
cooker. A fresh Ed25519 key signed the transcripts through cryptography; only
the public key and signed artifacts were retained. There is no fixture private
key or production trust material here.

`reference.json` fixes the exact scene bytes and expected artifact/build hashes.
Its source/settings digests are explicit byte sequences 00–1f and 20–3f, and its
provenance strings identify the reference rather than claiming an actual source
cook. Python and C++ both verify these same immutable binary fixtures. C++ uses
libsodium, independently of the Python cryptographic implementation. Malformed
fixtures are derived in temporary test directories, using disposable keys where
a valid signature is necessary to reach structural/semantic checks.
