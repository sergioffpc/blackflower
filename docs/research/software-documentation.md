# Useful software documentation

This source review supports documentation decisions for software maintainers.
The recommended approach is to document purpose, observable contracts and
essential rationale, while letting readable code express implementation steps.
The [C++ documentation policy](../cpp-guidelines.md#code-documentation) applies
these principles to project code.

## What the sources recommend

The C++ Core Guidelines distinguish redundant narration from useful intent. NL.1
discourages comments that merely restate code; NL.2 recommends explaining
intent; NL.3 recommends concise comments. Clear code therefore reduces the need
for prose without establishing that every important fact is visible in an
implementation. [C++ Core Guidelines, NL.1–3][core-comments].

Google distinguishes interface documentation from implementation commentary.
Declarations describe behavior and correct use; definitions explain operation.
Relevant interface information includes retained references, state changes and
synchronization assumptions. Google expects comments on almost all function
declarations except simple, obvious ones, including private functions. A blanket
ban on comments would depart from that guidance. [Google C++ comments][cpp].

For readability, Google recommends identifying the audience and scope, answering
essential questions near the beginning, and organizing material around reader
needs. Short sentences should develop one idea and omit unnecessary words.
[Google Technical Writing: documents][documents] and [short
sentences][sentences].

Diátaxis distinguishes learning, completing a task, looking up facts and
understanding reasons. Reference and explanation serve different reading needs;
mixing them indiscriminately makes the document's purpose harder to maintain.
[Diátaxis][diataxis].

For understanding a codebase, arc42 recommends describing building blocks
through responsibilities, interfaces and source locations. Deeper descriptions
should focus on relevant complexity rather than exhaustively describing every
internal element. [arc42 building block view][arc42].

Google's documentation guidance favors a small, accurate collection, updates in
the same change as code, removal of stale content and links to existing guidance
instead of duplication. [Google documentation best practices][maintenance].

## Recommended allocation of information

Use the existing architecture, contracts, glossary and decision records. Apply
the source distinctions within that structure; another documentation hierarchy
or generator is unnecessary for these recommendations.

-   **Code:** express operations, relationships and locally enforceable rules
    through names, types and structure. Improve those before adding explanatory
    prose to compensate for avoidable ambiguity.
-   **Interface documentation:** state purpose, observable results, caller
    obligations and guarantees that declarations do not already communicate.
    Include ownership, lifetimes, effects, failure semantics and concurrency
    conditions only where they affect correct use. Place each contract with its
    declaration or in an explicitly linked authoritative reference.
-   **Implementation comments:** consider a short rationale when a maintainer
    needs information that code cannot express. Explain the reason for a
    non-obvious choice or necessary ordering; avoid narrating individual
    statements. Link substantial rationale to its owning document.
-   **Architecture and decisions:** describe responsibilities, collaborations
    and consequential trade-offs. Link to source entry points rather than
    reproducing declarations or execution line by line.
-   **Operational documentation:** keep prerequisites, actions and expected
    outcomes together. Link conceptual background without interrupting the
    procedure with a design discussion.

The distinction between contract and implementation is the durability test:
contract documentation should remain useful when the implementation changes but
its promised behavior stays the same. Explaining what an operation accomplishes
can add essential information even when explaining each statement would add
none. This is a synthesis of the interface and intent guidance above.

## Review criteria

Keep documentation when it answers a reader's concrete question and has a clear
authoritative location. Review whether a maintainer can identify a module's
responsibility, find its interface, understand its observable behavior and know
the conditions required to use it without tracing its internals.

Delete paraphrases of signatures, duplicated contracts and obsolete rationale.
Keep terminology consistent with the glossary. Review documentation alongside
behavior changes; check links and promises as well as formatting. Separate
current behavior from proposals and retain only decision context needed to
understand the chosen design. These criteria apply the sources' emphasis on
reader needs, selective detail and maintenance; they do not prescribe a comment
quota or additional tests.

[core-comments]:
    https://isocpp.github.io/CppCoreGuidelines/CppCoreGuidelines#nl1-dont-say-in-comments-what-can-be-clearly-stated-in-code
[cpp]: https://google.github.io/styleguide/cppguide.html#Comments
[documents]: https://developers.google.com/tech-writing/one/documents
[sentences]: https://developers.google.com/tech-writing/one/short-sentences
[diataxis]: https://diataxis.fr/
[arc42]: https://docs.arc42.org/section-5/
[maintenance]: https://google.github.io/styleguide/docguide/best_practices.html
