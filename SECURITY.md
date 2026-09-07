# Security policy

Blackflower is a C++23 simulation project in initial development. No released versions or supported deployment configurations exist yet.

## Supported revisions

Security fixes currently target the latest develop revision. There are no versioned maintenance branches or backport commitments. Define the supported release versions when the first release is prepared; the presence of main does not imply a released product.

## Reporting a vulnerability

This repository is public. Report a suspected vulnerability through **Security → Advisories → Report a vulnerability**, using GitHub's private vulnerability reporting. Do not disclose unremediated vulnerabilities in public issues or pull requests. Include:

- The affected commit, target platform, compiler, and dependency versions.
- Reproduction steps or a minimal example, expected behavior, and observed behavior.
- The suspected impact and relevant sanitized logs, including sanitizer output when available.

Keep credentials, personal data, and sensitive training material out of reports and attachments.

Maintainers assess the report, agree its scope, and track remediation and verification in the private advisory. Response and fix dates depend on the small team's capacity; no fixed service-level commitment is established. Preserve confidentiality while assessing and fixing the issue, and coordinate public disclosure after remediation.

## Repository security controls

The main and develop branches require pull requests, passing Linux validation, resolved review conversations, and signed commits. Force pushes and branch deletion are blocked; administrators are subject to these rules. See [the Git workflow](docs/git-workflow.md) for integration and the solo-developer review policy.

GitHub secret scanning and push protection detect supported credentials and block their introduction. Dependabot vulnerability alerts and security updates are enabled. [CodeQL](docs/static-analysis.md#codeql-security-analysis) analyzes C/C++ and GitHub Actions alongside clang-tidy and the sanitizer checks. These tools have limited coverage and do not replace review or establish that the code is vulnerability-free.

## Dependency maintenance

Dependabot checks GitHub Actions and the vcpkg baseline weekly, proposing changes against develop. It opens at most one version-update PR per ecosystem, keeping the total small enough for review by one or two developers. Its branches use the feature/dependabot prefix.

Review updates and their resolved dependencies, run the applicable Linux checks and Windows cross-build analysis, and verify signed Conventional Commits before integration. Preserve action SHA pins. A vcpkg baseline update also requires reviewing the local checkout instructions and resolved-version documentation. The baseline update is not an audit of every transitive dependency.

The vendored AI Hero skills, host compiler and tools, and Windows SDK require explicit reviewed updates. Automated version updates do not establish complete vulnerability coverage. Dependabot security updates use the repository's default branch independently of these develop-targeted version-update settings.
