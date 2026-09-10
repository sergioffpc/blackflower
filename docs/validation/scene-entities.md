# Scene entity validation

Scope: [#58](https://github.com/sergioffpc/blackflower/issues/58), against
develop baseline `bcc053d` with this delivery. The owner refined the contract to
use `Bounds` and only entities in USD and internal scenes.

## Method and expectations

The agreed seam is the installed cooker CLI, signed artifacts and actual C++
content harness, using independently provisioned disposable public-key trust.
The implementation was observed failing first against the previous Scenario
parser, then against the previous runtime scene encoding, before the new path
passed.

The reference fixtures contain a floor and a box entity. The box definition has
a unit body and a half-size cap translated locally by (1, 0, 0). Its placement
is (2, 1, 3), rotated 90 degrees around Y and scaled by two. Expected body
dimensions are (2, 2, 2); cap dimensions are (1, 1, 1) and its world centre is
(2, 1, 1). The floor dimensions are (20, 0.2, 20) with centre (0, -0.1, 0).
These analytical expectations do not use visual mesh bounds. Comparison
tolerances are 1e-9 metres and 1e-12 per quaternion component.

Normal workflow coverage checks:

-   All three role packs retain entity IDs, placement and owned oriented bounds
    after the complete source directory is removed.
-   Renaming a placement preserves its ID; another placement reuses the same
    definition with a distinct ID and its own bounds.
-   Bounds and Entities scopes may be omitted. Internal scenes contain only
    entities.
-   Copying the input tree to another absolute directory yields identical packs
    with the same signing key; editing a definition changes build identity.
-   Independently encoded signed fixtures load through Python and C++, and
    decoded entity storage survives caller changes, copies and moves.
-   The retained #22 integrity matrix rejects tampering, malformed manifests,
    unknown bound kinds and every truncated entity/bound record prefix without
    publishing partial content.

The previous primitive fixtures were regenerated independently from literal
scene/pack fields with an ephemeral signing key; no private key is retained. The
schema evolves in place, so previous development packs must be regenerated.

## Checks and limitations

Validation uses the development container, Clang 21.1.8, Python 3.14.4 and the
locked Python/vcpkg dependencies. Results below are local evidence for this
delivery, not remote CI or reference-hardware measurements.

The checks cover content preparation and consumption. They do not establish
rendering, physical interaction, destruction, ballistics or training fidelity.
Visual import remains in #23 and #59. Runtime consumers need no USD, Python,
definition files or private signing key. Source capture is per file, so authors
must keep inputs unchanged during cooking.

On 2026-09-10, Linux Debug analysis/formatting passed, followed by all ten CTest
entries after a test annotation correction. Linux TSan and Release each passed
the complete `check` target with ten CTest entries. The content suite contains
fifteen methods, including the retained integrity matrix. Pyink, Pylint, mypy
and shared source-style checks passed with no enabled diagnostics; no sanitizer
diagnostics occurred.

Windows Release cross-compilation and the complete `analyze` target also passed
with the prepared SDK via `XWIN_ROOT`. Windows execution remains unavailable
because this container exposes neither Windows executable interoperability nor
`wslpath`; cross-build success is not Windows runtime evidence. The final
isolated-filename integration adjustment passed the four native content checks.

## Review

Standards review initially found a contradictory historical ADR, an outdated
provenance parameter contract and a redundant validation forwarding helper. The
ADR now explicitly records the replacement contract, provenance documents its
dependency transcript input, and verification calls the decoder directly. The
separate follow-up Standards review reported zero remaining findings.

The independent Spec review reported zero material findings against #58 and the
owner's Bounds/entities-only refinements. Manual review covered identity
ownership, exact source capture, geometry composition, error categories, module
imports and the distinction between implemented and pending content.
