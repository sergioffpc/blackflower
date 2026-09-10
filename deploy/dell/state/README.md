# Dell operational state

This permanent `gitops` branch contains only desired cluster state. Source
workflows, bootstrap tools and templates belong to the ordinary source branches
under `deploy/dell`. Never merge a source branch into this branch.

Flux reads this public branch over HTTPS without GitHub credentials. The
operator writes signed Conventional Commits, runs `git verify-commit HEAD`, then
pushes a fast-forward update. Do not force-push or delete this branch. GitHub
must trust the operator's signing key before enabling required signed commits in
branch protection. Keep source branch protections unchanged.

`controllers` and `network` have pruning disabled to preserve infrastructure.
`environments` has pruning enabled and owns only exclusive workload resources.
Remove an environment from its Kustomize resource list to remove its workload,
Service and namespace. ExternalDNS then removes its owned DNS records and
MetalLB releases the address. Never put shared packs in an environment
namespace.

Do not add Actions workflows here. This branch never represents an environment
and must never trigger source or server builds. Automatic publication and state
writers are separate work under issue #44.

See the source branch's `docs/dell-operations.md` for setup, checks and
recovery.
