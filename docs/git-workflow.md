# Git workflow

Use [Git-flow](https://nvie.com/posts/a-successful-git-branching-model/), with main as the release branch and develop as the integration branch.

| Branch | Start from | Integrate into | Purpose |
| --- | --- | --- | --- |
| main | Existing repository history | — | Released versions. The initial repository setup predates the first release. |
| develop | main during initialization | A release branch when preparing a release | Validated work for the next release. |
| feature/<description> | develop | develop | Features, normal fixes, build work, and documentation. |
| release/<version> | develop | main and develop | Release preparation, stabilization, and release metadata. |
| hotfix/<version> | The affected release on main | main and develop | Urgent fixes to a released version. If a release branch is active, also carry the fix into it. |

Keep feature branches small and short-lived, within the Kanban work limits. Review and validate changes before integration. Record the review against the originating request or issue; a pull request can hold that review when used.

## Starting work

With a clean working tree, update develop and create a feature branch:

```sh
git switch develop
git pull --ff-only origin develop
git switch -c feature/example
```

Use a descriptive branch name and include an issue number when one exists. Work on feature branches rather than committing ordinary changes directly to main or develop.

## Integrating a feature

Run the [build checks](build.md#verification) and review the diff. Incorporate any relevant changes from develop, then validate the result before integration. Preserve the feature's history with a signed merge commit:

```sh
git switch develop
git pull --ff-only origin develop
git merge --no-ff -S feature/example -m "feat: integrate example"
git verify-commit HEAD
git push origin develop
```

Choose a Conventional Commit type and description that match the actual work. All source commits and merge commits must be signed. Retain branches needed for review; remove a completed feature branch only after its work is safely integrated.

## Releases and hotfixes

Create release branches only for an identified version and agreed release scope. Complete validation, merge the release into main with a signed merge commit, and create a signed annotated tag named v<version>. Merge the release fixes back into develop as well.

For a hotfix, start from the affected release, validate the correction, integrate it into main, and tag the corrected version. Carry the correction into develop and any active release branch before considering the hotfix finished.

Use meaningful Conventional Commit messages for release and hotfix merges. Verify commit and tag signatures before publishing. Creating a build scaffold does not itself create a release.

## Tooling

Git-flow is a branch workflow and can be followed with standard Git commands. The optional git-flow command-line extension uses these repository-local settings: release branch main, integration branch develop, prefixes feature/, release/, hotfix/, support/, and version-tag prefix v. Local Git configuration is not transferred by cloning; configure the extension in each checkout if using it.

The [agent workflow](agents/workflow.md) controls how work is specified and reviewed; this document controls where that work is committed and integrated. Commit conventions remain in [AGENTS.md](../AGENTS.md).
