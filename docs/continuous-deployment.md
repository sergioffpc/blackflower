# Simulation server continuous deployment

Status: accepted initial design. Diagnostic infrastructure is provisioned; the
application deployment pipeline and simulation-server Kubernetes resources
remain unimplemented. [Dell operations](dell-operations.md) records
infrastructure configuration and the completed LAN acceptance evidence. The
[CD specification](https://github.com/sergioffpc/blackflower/issues/44) in
GitHub Issues is the authoritative delivery specification; this document records
the supporting deployment design. [ADR-0012](adr/0012-use-flux-for-lan-cd.md)
records the delivery architecture and its trade-offs.

## Agreed behavior

Deploy the authoritative simulation server to Kubernetes with access restricted
to the owner's LAN. Each environment has its own DNS name.

| Source      | Environment                       | Update trigger | Removal trigger |
| ----------- | --------------------------------- | -------------- | --------------- |
| `develop`   | Permanent development environment | PR merge       | None            |
| `main`      | Permanent production environment  | PR merge       | None            |
| `feature/*` | Temporary feature environment     | Branch push    | Branch deletion |
| `hotfix/*`  | Temporary hotfix environment      | Branch push    | Branch deletion |
| `release/*` | Temporary release environment     | Branch push    | Branch deletion |

Feature environments use the GitHub Issue number as their ID. Hotfix and release
environments use their version. Branch names are
`feature/<issue>-<description>`, `hotfix/<version>` and `release/<version>`;
corresponding DNS labels are `feature-<issue>`, `hotfix-<version-with-hyphens>`
and `release-<version-with-hyphens>`. Allow one active branch per type and ID.
The mandatory feature ID convention applies to deployed feature branches under
the [Git workflow](git-workflow.md#deployment-state-branch). Use the DNS suffix
`blackflower.home.arpa`, with `develop` and `production` labels for the
permanent environments.

The infrastructure runs on the owner's Dell R630, with single-node K3s installed
directly on Debian and MetalLB allocating environment addresses. Each
environment receives a separate LAN IP and uses the same UDP port; its DNS name
resolves to that IP. Reserve the environment address pool outside DHCP. The
selected BIND DNS service, reserved IP pool and UDP 27015 are recorded in
[Dell operations](dell-operations.md). The diagnostic lifecycle passed from the
Lenovo through its normal Google Mesh resolver.

Run one simulation server instance per environment without autoscaling. Measure
the server before assigning resource limits. If capacity is insufficient, leave
the new environment visibly pending rather than removing another environment.

Deploy a server revision only after all required checks pass. A revision that
fails these checks must leave an existing environment on its previous version; a
new environment waits for a passing revision before its first deployment. If the
replacement fails to start, report a failed deployment. Recovery is a manual
restoration of the last working image and environment configuration; the
environment may remain unavailable until the operator intervenes. Automatic
rollback is outside the initial scope.

Use Flux inside the cluster for deployment. Publish public server images to GHCR
associated with this repository, and reference images by immutable digest.
Images contain the server and its dependencies; packs and credentials are
excluded. Public image access does not change LAN-only server access.

Keep deployment workflows and templates in the ordinary source branch flow.
Store desired environment state on a permanent `gitops` branch in this same
repository. Automation writes signed Conventional Commits to that branch without
a PR per deployment. Updates to `gitops` do not trigger server builds and do not
create an environment for that branch. Flux reconciles this branch into the
cluster. This operational branch has a distinct role from source branches; see
the [Git workflow](git-workflow.md#deployment-state-branch).

Server publication and content-pack publication have independent lifecycles.
Several server versions may use the same pack. Server replacement does not
require publishing a new pack. The server selects its pack through a
command-line argument, and packs reside on a filesystem shared by all server
pods. Store packs on the Dell and mount them read-only in server pods. A
separate publication operation writes packs while preserving files used by
running servers. Configure the pack argument per environment; new temporary
environments start with a predefined reference pack. Subsequent server
deployments preserve that selection. An explicit pack-selection configuration
change restarts the server. The storage mount mechanism, reference pack and
publication procedure remain open. Windows client build and distribution are
outside this CD scope; this work delivers only the server.

All environments, including production, restart on deployment and may disconnect
players. There is no session draining or 30-minute waiting policy in the current
scope. Temporary environment removal may also interrupt players. Publish the
most recent available approved revision for the branch and discard superseded
deployment attempts. A delayed check or build completion must not replace a
newer deployed revision with an older one. Intentional manual recovery is
distinct from ordinary automatic deployment ordering.

Deleting a temporary branch removes its server, exclusive configuration, network
service and DNS record, and releases its environment IP. Preserve shared packs
and published images. Reconcile environments against existing branches every 15
minutes to recover missed or failed deletion handling. In-flight builds must not
recreate an environment after its branch has been deleted.

## Acceptance targets

These are agreed targets awaiting implementation and measurement. Timing assumes
that the Dell, GitHub and registry are available and sufficient capacity exists.

| ID     | Stimulus                                              | Expected result                                                                                                                |
| ------ | ----------------------------------------------------- | ------------------------------------------------------------------------------------------------------------------------------ |
| CD-Q01 | All required checks pass for a deployable revision.   | Publish the image and make the server ready within 10 minutes.                                                                 |
| CD-Q02 | A branch deletion event is received.                  | Remove its temporary environment, DNS record and IP allocation within 5 minutes, preserving shared packs and published images. |
| CD-Q03 | Ordinary branch deletion handling is missed or fails. | Reconcile existing branches every 15 minutes and recover the stale environment cleanup.                                        |
| CD-Q04 | A new server starts.                                  | Report deployment success only after pack validation and readiness to accept connections.                                      |

Expose the source revision, image identity, selected pack and deployment state
through operational tooling. A custom operational interface is outside scope.

## Server container base

Use `ubuntu:26.04` as the base of the CD server image. Package the server and
its required runtime dependencies on that base, preserving compatibility with
the Dell-specific build. The Dell host continues to run Debian.

## Dell-specific server build

Every CD server image must contain code compiled specifically for the actual
Dell R630 hardware, prioritizing measured runtime performance. The
[reference specification](https://github.com/sergioffpc/blackflower/issues/11)
records two Intel Xeon E5-2690 v4 processors. Intel identifies this processor as
[Broadwell](https://www.intel.com/content/www/us/en/products/sku/91770/intel-xeon-processor-e52690-v4-35m-cache-2-60-ghz/specifications.html).
Confirm the Dell's actual exposed CPU features before accepting the target
build.

Record compiler version, target CPU, optimization settings and dependency build
configuration with the image identity. Preserve compatibility with the
deployment container's runtime libraries and validate execution on the Dell.
Compare representative simulation performance before claiming an improvement;
generic CI success alone does not establish target-hardware compatibility or
performance. The deployed binary is not required to run on older CPU targets.

The accepted initial profile is a Clang Release build on GitHub runners with
`-O3`, `-march=broadwell`, `-mtune=broadwell` and ThinLTO, including appropriate
source-built server dependencies. Keep numerical semantics intact; `-Ofast` and
`-ffast-math` are excluded. Validate the resulting artifact on the Dell. Defer
profile-guided optimization until representative simulation profiles can be
collected on that machine, and measure its benefit before adopting it. Never use
an unrelated CI runner's `-march=native` result as the Dell target. See the
[Clang compiler manual](https://clang.llvm.org/docs/UsersManual.html) and
[ThinLTO documentation](https://clang.llvm.org/docs/ThinLTO.html).

## Delivery flow

1.  An eligible source event starts validation for its exact revision. All
    required checks must pass before the revision can be deployed.
2.  GitHub Actions builds the Dell-specific Release server, publishes the server
    image to GHCR and obtains its digest.
3.  Automation verifies branch existence and revision ordering, then updates the
    environment's desired image on `gitops`, preserving its pack selection.
4.  Flux reconciles desired state. The environment restarts and becomes
    successful only after pack validation and server readiness.
5.  Branch deletion removes the temporary environment's desired state. Flux and
    the DNS integration remove its resources. Periodic branch reconciliation
    recovers missed cleanup without deleting shared packs or published images.

Manual recovery must restore desired state on `gitops` so reconciliation retains
the operator's chosen working revision.

## Implementation and provisioning inputs

-   Select the storage mount mechanism, reference pack and independent
    publication and trust provisioning procedure, preserving the existing
    signature and compatibility contracts.
-   Implement signed automated state updates, scoped write credentials, branch
    stale-run handling and manual recovery that remains stable under Flux
    reconciliation. The operational branch and its protection are already
    provisioned. Resolve branch deletion/recreation races and concurrent updates
    before deployment.
-   Extend the pinned infrastructure manifests and Flux reconciliation to
    simulation-server environments and validate the CD acceptance targets.
-   Define resource isolation, measure resource requirements and select
    operational status reporting mechanisms.

The proposed transport is UDP through GameNetworkingSockets; DNS names alone do
not establish per-environment packet routing. Hostname resolution in the client
is also unimplemented. The existing content contract requires immutable backing
files while packs are mapped; replacements must preserve that property.

See the [architecture deployment view](architecture.md#7-deployment-view),
[Git workflow](git-workflow.md), [C4 design](c4.md) and
[mapped content decision](adr/0011-map-content-files.md).
