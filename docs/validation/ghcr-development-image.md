# GHCR development image validation

## Scope

[#42](https://github.com/sergioffpc/blackflower/issues/42) separates development
image publication from ordinary native validation. The publisher builds the
locked Dockerfile and pushes a candidate to GHCR. CI reads the same immutable
reference as VS Code from devcontainer.json and checks the image's source-input
fingerprint before preparing project dependencies. There is no build fallback.

The initial candidate uses the snapshot download repair shared with PRs 40
and 41. The [snapshot checks](development-container-snapshots.md) preserve all
182 package versions and hashes. The
[snapshot repair run](https://github.com/sergioffpc/blackflower/actions/runs/34402322696)
built all three native images from fresh downloads of 182 packages and passed
all nine CTest entries per preset on PR 40. This establishes the bootstrap
recipe, not GHCR consumption. Content loader tests remain in their original PR.
The owner also requested matching extension lists: both editor configurations
include the same seven extensions, including the two GitHub recommendations from
PR 40.

## Local checks

The complete source style check covers the workflow, shell, JSON and Markdown
changes. Independent Standards and Spec reviews found no blocking issues in the
publication and consumption design. Signed commits are verified before push.

A temporary filesystem copy exercised image-input identity: all 11 listed inputs
individually changed the fingerprint when their contents changed, while an
unchanged copy produced the same value. The consumer's actual workflow shell was
executed with a Docker stub. It accepted a digest with matching input identity,
rejected an incorrect identity before tagging the image, and rejected a mutable
tag before invoking Docker. These probes validate the guard's control flow; they
do not establish registry connectivity or Docker behavior.

Docker and the VS Code UI are unavailable in the editing container. Hosted
publication and native checks must establish the registry and runtime boundary;
a successful local style check alone is insufficient. The editor's `image`
property is also the exact reference read by CI, but interactive VS Code startup
remains a separate manual check.

## Hosted acceptance

The
[initial publication](https://github.com/sergioffpc/blackflower/actions/runs/34403008727)
succeeded from source `8e653f1`. Its reference artifact supplied the committed
image digest:

```text
sha256:969cd756ccaca295b3633c75f899b6d8fec0f88e6afc3aedd767ba2d9c367ba6
```

The image labels record the source revision and input fingerprint. The final
consumer configuration uses this published reference.

Require all existing native Debug, TSan and Release checks and CodeQL checks on
the proposed revision. Native jobs must pull the pinned image on fresh hosted
runners, verify its input fingerprint, prepare locked project dependencies, then
complete fresh offline builds with compiler and vcpkg binary-cache reuse
disabled. The image must not be constructed in these jobs. Retain their normal
validation artifacts as evidence.

## Remaining boundaries

GHCR availability, package permissions and retained digests remain required for
fresh pulls. The initial package is private unless its visibility is changed in
GitHub; developers need read access and host Docker authentication. Forks
without package access require public visibility. Candidate rebuilds still
depend on Ubuntu snapshots and tool archives. No package mirror or independent
image backup is provisioned by this change. Registry publication is not
equivalent to promotion: merge only after the existing native checks pass.
