# Git workflow

Use [Git-flow](https://nvie.com/posts/a-successful-git-branching-model/), with
main as the release branch and develop as the integration branch.

| Branch                  | Start from                   | Integrate into                            | Purpose                                                                                        |
| ----------------------- | ---------------------------- | ----------------------------------------- | ---------------------------------------------------------------------------------------------- |
| main                    | Existing repository history  | —                                         | Released versions. The initial repository setup predates the first release.                    |
| develop                 | main during initialization   | A release branch when preparing a release | Validated work for the next release.                                                           |
| `feature/<description>` | develop                      | develop                                   | Features, normal fixes, build work, and documentation.                                         |
| `release/<version>`     | develop                      | main and develop                          | Release preparation, stabilization, and release metadata.                                      |
| `hotfix/<version>`      | The affected release on main | main and develop                          | Urgent fixes to a released version. If a release branch is active, also carry the fix into it. |

Keep feature branches small and short-lived, within the Kanban work limits.
Review and validate changes before integration. Record the review against the
originating request or issue in the pull request.

## Operational state branch

The permanent `gitops` branch holds only deployed desired state, with
independent history from source branches. It is never merged into source history
or treated as a deployable environment. Keep templates, bootstrap tools and
workflows in the normal feature/develop/main flow. Do not add Actions workflows
to gitops.

Operational updates use signed Conventional Commits verified before a
fast-forward push, without a PR per deployment. Protect gitops against deletion
and force pushes and require GitHub-recognized signatures. The operator
provisions scoped credentials outside Git; Flux reads this public branch without
credentials. Preserve concurrent environments when updating state. See
[Dell operations](dell-operations.md) and
[ADR-0013](adr/0013-deploy-private-lan-services-with-flux.md). The automatic
state writer remains separate work under issue #44.

## Protected branches

GitHub protects main and develop, including administrators. Both require pull
requests, signed commits, resolved review conversations, and the following
checks from GitHub Actions against a revision up to date with the target branch:

-   `Ubuntu 26.04 / debug`
-   `Ubuntu 26.04 / tsan`
-   `Ubuntu 26.04 / release`
-   `CodeQL / c-cpp`
-   `CodeQL / actions`

Force pushes and branch deletion are disabled. Merge commits remain enabled to
preserve Git-flow history. A pull request is required even when working alone;
the enforced approval count is zero so the author can integrate reviewed work
without an unavailable second person. Follow the project's review process and
request the other developer's review for consequential changes when available.

These are server settings, not settings installed by cloning the repository.
Maintain them in GitHub's branch protection settings. The initial main baseline
predates the build infrastructure; release PRs must carry the workflows and pass
the checks before integration.

## Starting work

Install Git LFS (`sudo apt-get install git-lfs` on the reference Ubuntu host).
After cloning, prepare the
[style tools](style-guidelines.md#installation-and-commands) and activate the
versioned hooks:

```sh
git config --local core.hooksPath .githooks
git lfs install --local --skip-repo
git lfs pull
```

The versioned pre-push hook uploads LFS objects before publishing commits;
`--skip-repo` preserves the project's hook files while enabling the local LFS
filters. The binary test packs and public keys under tests/fixtures/packs use
the patterns in [.gitattributes](../.gitattributes). Keep their working copies
as binary files; Git stores LFS pointers in commits. Existing historical Git
blobs are retained. The build CI downloads LFS contents during checkout.

The [pre-commit hook](../.githooks/pre-commit) checks the complete staged source
snapshot against the formatting rules for C++, Python, JavaScript, Markdown,
JSON, and shell. It preserves partial staging: formatting the working copy does
not make an unformatted staged version acceptable. The same check runs before
automatic merge and `git am` commits. Missing tools or diagnostics reject the
commit. See the
[style acceptance policy](style-guidelines.md#acceptance-and-ci).

The [commit-msg hook](../.githooks/commit-msg) rejects commits whose subject is
neither a Conventional Commit nor a message beginning with `Merge` followed by a
space and a description. Conventional subjects use `type: description`, an
optional `(scope)`, and an optional `!` before the colon. A nonempty description
is required. Types use lowercase letters and digits, starting with a letter.
Bodies and footers remain available.

Conventional Commit messages remain preferred for project and PR merge commits;
Git-generated merge messages are also accepted. The hook does not rewrite
messages or sign commits. Keep the existing signed-commit configuration enabled.
Hooks run locally and can be bypassed with Git's `--no-verify`; this is not a
server-side commit-message rule. Every clone needs the setup command, which
replaces any existing `core.hooksPath` setting in that clone.

Run `npm --prefix tools/code_quality test` for staged-content rejection and
preservation through actual Git commits in a disposable repository.

With a clean working tree, update develop and create a feature branch:

```sh
git switch develop
git pull --ff-only origin develop
git switch -c feature/example
```

Use a descriptive branch name and include an issue number when one exists. Work
on feature branches rather than committing ordinary changes directly to main or
develop.

## Integrating a feature

Run the [build checks](build.md#verification) and review the diff. Incorporate
any relevant changes from develop with a signed merge, then validate the result
before integration. Push the feature branch and open a PR:

```sh
git verify-commit HEAD
git push -u origin feature/example
gh pr create --base develop --head feature/example --title "feat: add example" --body-file /tmp/example-pr.md
```

Write the PR body to the referenced file before running the command. Wait for
all required checks and complete the review. Merge through GitHub using a merge
commit with a Conventional Commit title, such as `feat: integrate example`;
GitHub signs its web-flow merge commits. Verify the resulting commit's signature
after fetching it. Do not push local merges directly to a protected branch.

Choose a Conventional Commit type and description that match the actual work.
All source commits and merge commits must be signed. Retain branches needed for
review; remove a completed feature branch only after its work is safely
integrated.

## Releases and hotfixes

Create release branches only for an identified version and agreed release scope.
Complete validation, merge the release into main through a PR with a signed
merge commit, and create a signed annotated tag named `v<version>`. Merge the
release fixes back into develop through a PR as well.

For a hotfix, start from the affected release, validate the correction,
integrate it into main through a PR, and tag the corrected version. Carry the
correction into develop and any active release branch through PRs before
considering the hotfix finished.

Use meaningful Conventional Commit messages for release and hotfix merges.
Verify commit and tag signatures before publishing. Creating a build scaffold
does not itself create a release.

## Deployment state branch

The [accepted CD design](continuous-deployment.md) adds a permanent `gitops`
branch in this repository for desired environment state. This is an operational
branch, not a source integration or release branch. The branch is provisioned
under the [operational state rules](#operational-state-branch); the automatic
state writer remains unimplemented.

Deployment workflows and templates follow ordinary feature, develop and main
review. State-writing automation may update `gitops` directly without a PR per
deployment, using signed Conventional Commits and verifying signatures before
push. This exception applies only to operational state updates; it does not
change main or develop protection or permit direct source integration.

Flux watches `gitops`. Its updates must not trigger the server build/CD source
flow, and the branch must not create its own environment. Keep it out of normal
release merges and temporary-branch cleanup. Manual recovery restores desired
state on this branch to prevent Flux from reversing the recovery.

When application CD is enabled, deployed feature branches must use
`feature/<issue>-<description>`, with one active feature branch per Issue ID.
Hotfix and release branches retain `hotfix/<version>` and `release/<version>`.
The automation must check source validation, branch existence and revision
ordering before changing an environment. See
[ADR-0012](adr/0012-use-flux-for-lan-cd.md) for the trade-off and consequences.

## Tooling

Git-flow is a branch workflow and can be followed with standard Git commands.
The optional git-flow command-line extension uses these repository-local
settings: release branch main, integration branch develop, prefixes feature/,
release/, hotfix/, support/, and version-tag prefix v. Local Git configuration
is not transferred by cloning; configure the extension in each checkout if using
it.

The [agent workflow](agents/workflow.md) controls how work is specified and
reviewed; this document controls where that work is committed and integrated.
Commit conventions remain in [AGENTS.md](../AGENTS.md).
