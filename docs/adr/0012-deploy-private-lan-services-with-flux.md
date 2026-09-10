# Deploy private LAN services with Flux

Status: accepted by the owner in
[issue #44](https://github.com/sergioffpc/blackflower/issues/44). Infrastructure
implementation and evidence are tracked separately in
[issue #45](https://github.com/sergioffpc/blackflower/issues/45).

Use single-node K3s directly on the Debian Dell R630, with Flux pulling desired
state from a permanent gitops branch in the existing public repository. MetalLB
supplies a distinct private LAN IP per environment; LAN DNS uses
blackflower.home.arpa and every service uses a common UDP port. This avoids
exposing the Kubernetes API to hosted CI and avoids a second repository.

Keep source workflows and templates in the ordinary Git-flow branches.
Operational state updates use verified signed Conventional Commits without a PR
per deployment; the operational branch creates neither an environment nor
recursive source builds. Source protections remain unchanged. Scoped writer
credentials are provisioned operationally and never published.

The agreed later pipeline publishes digest-addressed public Ubuntu 26.04 server
images after required checks, compiled for the Dell's confirmed Broadwell CPUs.
Run one server per environment and restart on updates. Shared signed scenario
packs remain independent of images, read-only, and selected by command-line
argument. Environment cleanup preserves shared packs, images and other
resources. Manual recovery changes desired state. These server and automation
requirements remain future work; a diagnostic service cannot satisfy them.

For Google Mesh, use a BIND resolver on the Dell with authenticated RFC2136
updates from ExternalDNS. TXT ownership isolates managed records, and Flux
pruning removes only exclusive environment resources. Running DNS outside K3s
allows resolution during cluster recovery, but the Dell remains a LAN DNS
availability dependency. [Operations](../dell-operations.md) records setup,
package policy, privacy controls and recovery.
