# Agent development workflow

Use the [AI Hero skills workflow](https://www.aihero.dev/skills) with the project's [arc42 architecture](../architecture.md). Read the installed SKILL.md for each stage when using it. Resume from the stage supported by the existing artifacts and the user's request.

Follow the [development process](../development-process.md) for Kanban work limits, selected XP practices, and fidelity validation alongside the stage guidance below.

## Main flow

| Stage | Skill | Result |
| --- | --- | --- |
| Clarify | /grill-with-docs | Agreed intent, resolved terminology, and significant decisions. |
| Specify | /to-spec | A specification in GitHub Issues with behavior, scope, and testing decisions. |
| Slice | /to-tickets | Small, independently verifiable tickets with explicit blockers. |
| Implement | /implement | Working behavior, tests, and updated documentation. |
| Review | /code-review | Findings against repository standards and the originating specification. |

Agree test boundaries during specification. Review ticket granularity and dependencies before publishing the breakdown. Work on tickets whose blockers are resolved. During implementation, use test-driven development where applicable at the agreed boundaries, run relevant checks, and resolve review findings before reporting completion.

For documentation-only changes, validate accuracy, links, and formatting. Reuse existing specifications or tickets when they already describe the authorized work.

## Supporting skills

Use /wayfinder for a large effort with unresolved decisions, /research for evidence, and /prototype for design experiments. Use /triage for incoming issues, /diagnosing-bugs for failures, /improve-codebase-architecture for structural improvements, and /resolving-merge-conflicts during a conflicted merge or rebase. Use /ask-matt when the next skill is unclear.

## Integration with arc42

Keep requirements and constraints in sections 1–3, the solution and views in sections 4–8, decision links in section 9, measurable quality scenarios in section 10, risks in section 11, and the glossary link in section 12. Update affected sections alongside the implementation; link to issues, ADRs, and source files instead of duplicating their contents.

Completion follows arc42 section 8. Language and signed Conventional Commit rules remain defined in [AGENTS.md](../../AGENTS.md). Respect the requested delivery scope: for review-only work, leave changes local and uncommitted even when a skill normally ends with a commit.
