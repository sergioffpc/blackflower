# Use Flux and a same-repository branch for LAN deployment

Status: accepted by the owner. Diagnostic infrastructure is provisioned under
[ADR-0013](0013-deploy-private-lan-services-with-flux.md); application
deployment automation remains pending.

The simulation server needs automatic permanent and temporary environments on
the owner's LAN, while source validation runs in GitHub. Use Flux on single-node
K3s on the Debian Dell R630, with MetalLB providing environment IPs, public
digest-pinned server images in GHCR and desired state on a permanent `gitops`
branch in the application repository.

Base CD server images on `ubuntu:26.04`; the Dell host remains on Debian.

The owner selected Flux after considering Argo CD and chose one repository over
a separate deployment repository. The dedicated state branch separates automatic
operational updates from source integration and avoids recursive server builds.
Workflows and templates retain normal source review; state updates use signed
Conventional Commits without a PR per deployment. Cluster-side reconciliation
fits LAN-only access without requiring a public Kubernetes API for hosted CI.

Server and pack publication remain independent. Server pods read shared local
packs selected by command-line argument; server images contain no packs or
credentials. Every environment restarts on deployment, including production, to
keep the initial delivery simple. This accepts interrupted sessions, shared-host
outages and manual recovery after failed replacement instead of implementing
session draining, high availability or automatic rollback.

Build server artifacts on GitHub for the Dell's specified Xeon E5-2690 v4 CPUs
using Clang Release, explicit Broadwell targeting and tuning, and ThinLTO.
Prioritize measured performance on the Dell over compatibility with older CPUs,
while preserving numerical semantics. Actual target execution remains required;
PGO waits for representative workloads and measured benefit. The
[build contract](../continuous-deployment.md#dell-specific-server-build) defines
the agreed flags and evidence.

The state-writing automation requires scoped permissions and commit signing.
Manual recovery must change desired state so Flux does not undo it. Branch
deletion and stale build completion must converge on removal, while shared packs
and published images survive temporary-environment cleanup. Detailed behavior,
acceptance targets and provisioning inputs are in the
[CD design](../continuous-deployment.md).
