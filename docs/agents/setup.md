# AI Hero skill setup

The repository includes all 25 skills published in the
[AI Hero catalog](https://www.aihero.dev/skills). Their editable files and
supporting resources are committed under [.agents/skills](../../.agents/skills),
so a clone contains the same workflow without depending on a developer's global
installation.

## Provenance

-   Source: [mattpocock/skills](https://github.com/mattpocock/skills).
-   Revision: 3cca18b368ae95cdbdebbff572ccafa662551015.
-   Included: the 18 engineering and seven productivity skills in that revision.
-   License: [upstream MIT license](../../.agents/skills/AIHERO-LICENSE).
-   Inventory and SHA-256 file hashes: [skills-lock.json](skills-lock.json).

The vendored skill files are unchanged from upstream. The lock file records the
flattened local paths. Experimental and miscellaneous upstream collections are
outside the published catalog installed here.

## Using a new clone

Open the repository root with an agent that supports project skills. Read
[AGENTS.md](../../AGENTS.md) and the [workflow](workflow.md). In Codex, the
project skill directory is .agents/skills; the skills become available on the
next turn after installation. If an existing session retains an old catalog,
start a new session from this checkout and check the skill picker. Other agents
may require their own skill-directory configuration.

Use the local /ask-matt skill to choose a stage, or invoke the stage by name. In
Codex CLI and its IDE extension, select a skill through /skills or mention it as
$ask-matt; slash-prefixed names below are workflow labels. If a global copy has
the same name, choose the repository path to use the pinned definitions. The
main flow is /grill-with-docs → /to-spec → /to-tickets → /implement →
/code-review. Read each stage's SKILL.md before using it; helper files remain
beside their skill.

The repository-specific setup has already been completed:

| Setting              | Authoritative configuration                                                                              |
| -------------------- | -------------------------------------------------------------------------------------------------------- |
| Issue tracker        | [GitHub Issues](issue-tracker.md), accessed through an authenticated gh CLI.                             |
| Triage roles         | [Five default labels](triage-labels.md).                                                                 |
| Domain documentation | [Single-context layout](domain.md): root CONTEXT.md and docs/adr/ when domain terms and decisions exist. |
| Process              | [Kanban, XP, and validation](../development-process.md).                                                 |
| Integration          | [Git-flow and signed Conventional Commits](../git-workflow.md).                                          |

Cloning does not require rerunning /setup-matt-pocock-skills. Use that skill
when changing tracker or domain layout; preserve the existing agreed
configuration unless requirements change. Authentication and secrets stay
outside the repository.

## Installation and updates

The vendored copy has local style adaptations: wrapped prose, consistent
headings and lists, fenced template examples, and formatted shell helpers.
`skills-lock.json` retains the upstream source revision and hashes the current
local files. After editing or formatting a skill, refresh its file hash and run
the [source style checks](../style-guidelines.md#installation-and-commands).
Retain these adaptations when reviewing an upstream update.

The initial installation used the skill-installer helper with the source
repository, explicit revision, selected skill paths, and this repository's
.agents/skills directory as the destination. No global installation is needed to
use the committed copy.

For a reviewed update:

1.  Create a feature branch from develop and choose an explicit upstream commit.
2.  Download that revision into a temporary directory. Compare its engineering
    and productivity skills with the inventory in skills-lock.json; review
    additions, removals, changed instructions, and helper scripts.
3.  Copy each selected skill directory into `.agents/skills/<name>`, including
    its supporting files, and retain the upstream license. Remove obsolete files
    only after checking for local edits.
4.  Update the revision, inventory, and hashes in skills-lock.json. Update this
    guide and the project workflow if the catalog or invocation behavior
    changed.
5.  Verify that every listed skill contains SKILL.md, its local references
    resolve, and file hashes match. Review the change and commit it using the
    repository's signed Conventional Commit rules.

The upstream site's generic installer tracks its selected upstream revision. For
this project, review and commit updates explicitly so that collaborators keep
the same skill definitions.

## References

-   [AI Hero catalog and installation](https://www.aihero.dev/skills).
-   [Codex skill documentation](https://developers.openai.com/codex/skills/).
