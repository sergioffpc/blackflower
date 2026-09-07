# Issue tracker: GitHub

Specifications and tickets live in GitHub Issues. Use the gh CLI from this
checkout; infer the repository from the Git remote rather than hard-coding an
account name.

Workflow states and their distinction from triage labels are defined in the
[development process](../development-process.md#kanban-workflow).

## Operations

-   Create: `gh issue create --title "..." --body-file <file>`.
-   Read:
    `gh issue view <number> --json number,title,body,labels,comments,state`.
-   List: `gh issue list --state open --json number,title,body,labels,comments`,
    with appropriate state and label filters.
-   Comment: `gh issue comment <number> --body-file <file>`.
-   Label: `gh issue edit <number> --add-label "..."` or `--remove-label "..."`.
-   Close: `gh issue close <number>`.

Write multiline issue bodies and comments to a temporary file and pass it with
--body-file. Use the vocabulary in [triage-labels.md](triage-labels.md).

When a skill says to publish to the issue tracker, create a GitHub issue. When
it says to fetch a ticket, read its full body, labels, and comments.

## Pull requests as a triage surface

**PRs as a request surface: no.**

GitHub issues and pull requests share a number space. When a reference is
ambiguous, resolve its type before acting. Pull requests remain available for
code review.

## Ticket relationships and wayfinding

Link tickets to their source specification. Use native sub-issues for
parent-child relationships and native issue dependencies for blockers where
available. Otherwise, maintain a task list in the parent, a `Part of #<number>`
reference in each child, and a `Blocked by: #<number>` list in dependent
tickets.

For /wayfinder, use one issue labelled wayfinder:map with Notes,
Decisions-so-far, and Fog sections. Its child tickets use wayfinder:research,
wayfinder:prototype, wayfinder:grilling, or wayfinder:task as appropriate.
Create these labels when first needed.

Select an open, unassigned child whose blockers are all closed, in map order.
Claim it with `gh issue edit <number> --add-assignee @me`. Record the result,
close the completed child, and add a concise result and link to the map's
Decisions-so-far section.

Publishing child tickets does not complete or close their parent specification.
