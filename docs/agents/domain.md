# Domain documentation

## Layout and reading rules

This repository uses a single context: CONTEXT.md at the repository root and
architecture decision records in docs/adr/.

Before exploring the codebase, read CONTEXT.md and ADRs relevant to the work,
when present. Also read [the arc42 architecture document](../architecture.md).

If CONTEXT.md or docs/adr/ is absent, proceed without raising their absence as
an issue. Create them lazily when domain terms or significant decisions are
resolved. If a future migration introduces CONTEXT-MAP.md, follow it to the
contexts relevant to the task.

## Vocabulary

CONTEXT.md is the authoritative domain glossary. Keep it focused on terms and
definitions. Use its vocabulary in specifications, tickets, code, tests, and
documentation. Clarify ambiguous or conflicting terms before adding definitions.

arc42 section 12 links to the glossary once it exists, rather than copying
definitions.

## Decisions

Record decisions with meaningful trade-offs and lasting consequences in
docs/adr/, using numbered Markdown files. Capture status, context, the choice,
alternatives, and consequences.

arc42 section 9 indexes these records. When proposing something that conflicts
with an existing ADR, identify the conflict and explain why the decision should
be revisited. Preserve superseded decisions and link to their replacements.
