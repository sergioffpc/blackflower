# Development process

Status: approved. These operating rules apply to a team of one or two people alongside the repository conventions.

## Purpose and scope

Blackflower is intended to be a first-person simulation for military mission training, with physical, acoustic, and visual realism as product priorities. This process combines Kanban for managing work, selected Extreme Programming (XP) practices for engineering, the AI Hero skill workflow for agent collaboration, and arc42 for architecture documentation.

Develop a small, measurable experience first, then expand its scope as validation supports it. Implementation uses C++23 with Clang; specific training scenarios, learning objectives, hardware, the remaining technology choices, and fidelity thresholds remain to be defined.

## Responsibilities

The project owner prioritizes outcomes and resolves product trade-offs. Each active ticket has one responsible human, who may also implement it. Agents assist with research, implementation, tests, and review under that person's direction. With two people, share knowledge and review each other's consequential changes; pair on difficult design or implementation work when useful.

Seek feedback from representative users and relevant subject-matter experts when evaluating fidelity and training outcomes. Identify the evaluator and evaluation method for each such claim before accepting it.

## Kanban workflow

GitHub Issues holds specifications and tickets. Track each ticket through these states:

| State | Meaning | Exit condition |
| --- | --- | --- |
| Backlog | A request, defect, or question awaiting clarification or priority. | The readiness criteria below are met. |
| Ready | Scoped, verifiable work that can be selected. | Its blockers are resolved, a responsible human is assigned, and capacity is available. |
| In development | Implementation, investigation, or documentation is underway. | The output and evidence are ready for review. |
| In validation | Acceptance checks, fidelity evaluation, and review are underway. | The completion criteria below are met. |
| Done | The outcome has been verified and its evidence recorded. | Reopen if a failed acceptance criterion is discovered. |

Work starts when a ticket enters In development and finishes when it reaches Done. Failed validation returns the ticket to In development. Cancellation closes the issue with a reason and is recorded separately from completed delivery.

Use a GitHub Projects status field when a board is available. Until then, keep a `Workflow state: <state>` line in each ticket body. A board has not been provisioned by this document. Keep one authoritative state per ticket.

The [triage labels](agents/triage-labels.md) describe readiness and routing; they do not represent these workflow states. A ready-for-agent ticket may still have unresolved blockers.

### Work in progress

- Allow at most one active ticket per responsible human: one ticket for a solo developer, at most two for a two-person team. Count both In development and In validation.
- Agents and parallel tool runs do not increase the team's limit. Two people pairing on one ticket consume one ticket of capacity.
- Keep blocked active tickets within the limit and identify their blocker and next action. Prefer finishing, reviewing, or unblocking existing work before starting another ticket.
- For urgent work, explicitly pause an existing ticket and record the reason. Paused work still counts as active until deliberately returned to Backlog or Ready; retain its history and elapsed time.

### Readiness criteria

A delivery ticket needs a clear intended outcome, acceptance criteria, scope boundaries, dependencies, and an agreed verification method. Link its source specification and applicable architecture decisions. For a fidelity-sensitive change, identify the reference evidence, test conditions, and proposed acceptance thresholds before implementation.

Make tickets small enough to complete and validate independently. Split work into demonstrable behavior across the necessary layers. For mechanical changes that cannot be delivered independently, record a staged migration and its integration dependencies.

An investigation ticket instead needs a specific question, a fixed time budget, a planned experiment or source search, and the decision it will inform. It may be complete with an inconclusive result if the evidence, limitations, and next step are recorded. An implementation ticket requires its promised behavior to work.

## Engineering practices

These baseline practices are adapted from XP for this project.

| Practice | Project rule | Evidence |
| --- | --- | --- |
| Small deliveries | Deliver a narrow working increment with each delivery ticket. | A runnable build, reproducible demonstration, or reviewed document. |
| Test-driven development | For behavior with an agreed automated test boundary, write and observe a failing test, implement the behavior, then refactor. Test observable behavior. | Relevant tests pass and cover the stated acceptance criteria. |
| Continuous integration | Integrate small changes frequently. Automate the build and relevant checks when executable code is introduced. Resolve a broken shared build before extending it. | Results from the integrated revision; local checks are identified as local until CI exists. |
| Refactoring | Improve internal structure in small steps while preserving observable behavior. Separate intended behavior changes from structural changes when reviewing them. | Existing behavior checks remain green. |
| Coding conventions | Follow the [C++ engineering guidelines](cpp-guidelines.md), combining Google style, the C++ Core Guidelines, and modern adaptations of Effective C++. Apply the [clang-tidy policy](static-analysis.md) to C++ changes and automate it when the Clang toolchain is introduced. | Applicable formatting, static-analysis, and review results. |
| Sustainable pace | Plan against actual capacity and leave room for uncertainty, review, and learning. Reduce scope when work no longer fits. | The weekly review adjusts workload and priorities. |

Use the simplest design that satisfies current requirements and quality goals. Both developers may improve any module, using its tests and documented contracts. Use a common domain vocabulary and keep a person available to clarify product expectations.

Pair programming is optional for this small team. An agent can assist a solo developer, while responsibility for accepting its work remains with the human. For prototypes, documentation, visual output, and audio perception, choose suitable checks instead of manufacturing unit tests that do not demonstrate the result.

## Fidelity and validation

Before implementing a fidelity-sensitive change, define what will be compared and what difference is acceptable. Verification checks whether the implementation meets its specification; validation checks whether the specification and resulting simulation adequately represent the intended use.

| Area | Evaluation approach |
| --- | --- |
| Physics | Compare selected behavior with documented reference data or analytical cases under controlled conditions, using explicit tolerances. |
| Audio | Use repeatable listening scenes to assess spatial position, occlusion, reverberation, and synchronization. Record output-device configuration and listening conditions. |
| Visuals | Compare scale, materials, lighting, and animation against references using fixed scenes, camera settings, and documented evaluation criteria. |
| Performance | Measure frame-time distribution, stalls, and input response on declared hardware and settings against agreed budgets. |
| Training outcomes | Define learning objectives and evaluation criteria with representative users and relevant experts. Record the results separately from visual or physical fidelity. |

Store reusable reference scenes and automated checks with the project when introduced. Record the tested revision, reference source and version, configuration, expected result, observed result, and known limitations. Keep large measurements or captures in an appropriate artifact location and link them from the ticket.

Automated tests establish only the properties they check. Use human evaluation for perceptual judgments and evidence appropriate to each training claim. Compare affected reference scenes after changes; changing an accepted baseline requires an explanation of the intended improvement and a record of any trade-off.

When fidelity conflicts with performance or scope, measure the alternatives and make the trade-off visible. Resolve significant choices through the project owner, record lasting architectural decisions in ADRs, and update arc42 quality scenarios. Numeric targets are pending; this document does not establish fidelity or performance results.

## Cadence and feedback

- Each working day, inspect active work and blockers; no meeting is required for a solo developer.
- Each week, inspect a runnable increment or investigation results, collect available feedback, and adjust priorities. Discuss what slowed delivery or weakened quality and choose a concrete process improvement when needed.
- At each completed ticket, record its result and evidence. Record significant architectural learning when it occurs.

Track active ticket count, completed delivery tickets per week, age of unfinished tickets, and cycle time from start to finish. Keep investigations and cancelled work identifiable so they do not imply delivered product capability. Use these observations to improve flow and forecasting.

Initial service-level expectation: 85% of ordinary delivery tickets finish within five working days of starting. This is a planning assumption without historical evidence, not a delivery promise. Review it weekly and replace it with a forecast based on observed cycle times when sufficient representative data exists. Investigation tickets use their explicit time budgets.

## Completion criteria

A delivery ticket reaches Done when:

1. Its acceptance criteria pass and the result is independently reviewable.
2. Relevant automated checks, performance measurements, and human evaluations have been performed, with results linked to the tested revision or artifact. Checks that do not apply are identified with a reason.
3. The change has been reviewed against project conventions and its originating specification. Findings that prevent acceptance are resolved; other limitations are explicitly recorded.
4. Affected architecture sections, decision links, and operational documentation reflect the result. Remaining risks have a recorded next step.
5. The requested delivery action is complete. A review-only request ends with local changes and validation evidence; committing, pushing, or publishing follows the user's authorized scope and the repository's signed Conventional Commit rules.

For documentation-only work, check accuracy, internal links, consistency, and formatting. No application test suite is currently available. For investigations, apply the investigation exit criteria above and retain their evidence.

## Relationship to the agent workflow and arc42

Use [the AI Hero workflow](agents/workflow.md) to clarify, specify, split, implement, and review work. Existing artifacts can satisfy earlier stages. This process defines how work moves and is evaluated; the skill documents define how agents perform each stage.

Maintain product goals, constraints, views, decisions, quality scenarios, and risks in [the arc42 document](architecture.md). Keep the process rules here and link to them from arc42 section 8. Maintain the glossary and ADRs through [the domain documentation rules](agents/domain.md).

The first milestone is a small reference scene that exercises movement, physical interaction, audio, and lighting together. Agree its acceptance criteria and target hardware before building it, then use its evidence to decide what to expand next.

## References

- [The Kanban Guide](https://kanbanguides.org/the-kanban-guide/): workflow, work-in-progress controls, flow metrics, and forecasting.
- [Extreme Programming, Agile Alliance](https://agilealliance.org/glossary/xp/): XP practices and their evolution.
- [What is Extreme Programming?, Ron Jeffries](https://ronjeffries.com/xprog/what-is-extreme-programming/): engineering and collaboration practices.
- [AI Hero skills](https://www.aihero.dev/skills): the agent workflow.
- [The arc42 method](https://arc42.org/method/): iterative architecture work.
