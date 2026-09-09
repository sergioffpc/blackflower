# Invalid pack rejection

Scope: [#22](https://github.com/sergioffpc/blackflower/issues/22), tested from
the `f356016` develop baseline with the changes in this delivery. The existing
loader already implements rejection; this change adds observable acceptance
coverage without changing the byte format or production loading interface.

## Acceptance matrix

The Python integration driver invokes the actual C++ `LoadFile` boundary through
the content harness. It constructs bytes independently of the cooker encoder,
generates disposable Ed25519 keys in memory and verifies every constructed
signature with Python cryptography before using a signed malformed fixture. A
valid independently encoded control must load successfully.

| Input                                                                                | Expected result                                          |
| ------------------------------------------------------------------------------------ | -------------------------------------------------------- |
| Altered build identity, provenance, authenticated digest or signature                | Signature rejection                                      |
| Altered payload with unchanged authenticated digest                                  | Digest rejection                                         |
| Signed artifact carrying its signing key, with different independent trust           | Unknown signing key                                      |
| Missing file or truncated header, payload or signature; trailing data                | Mapping, length or layout error, as applicable           |
| Unknown magic/version, zero, multiple or maximal resource count                      | Unsupported format or resource count                     |
| Maximal u64 sizes, inconsistent sizes or undersized manifest                         | Layout rejection without overflowing arithmetic          |
| Empty, oversized or non-printable provenance string                                  | Provenance rejection                                     |
| Invalid resource identity/type/schema/reserved field, offset or size                 | Resource rejection                                       |
| Duplicate records with overlapping ranges and a valid signature                      | Unsupported resource count: v1 permits exactly one entry |
| Signed payload with incorrect authenticated digest                                   | Digest rejection                                         |
| Missing scene header, trailing scene data or maximal collection counts               | Scene length rejection                                   |
| Every truncated prefix of each box, sphere, point/directional light and spawn record | Scene length rejection                                   |
| Unknown full-width shape/light discriminator                                         | Unsupported kind                                         |
| Validly signed lights in ServerScene/AgentScene or spawns in AgentScene/ClientScene  | Scene contract rejection                                 |

Each rejection must exit one, emit no prepared-content stdout, and report the
expected operator diagnostic. A ten-second subprocess deadline also makes a
nonterminating parser fail the test. These assertions exercise the public
console contract; production callers continue to discriminate typed `PackError`
values rather than diagnostic strings.

The C++ pathname test loads a copied fixture, retains a pack copy after the
original is destroyed, and attempts to replace its path with an invalid file.
Linux must allow replacement while subsequent access to the existing pack still
returns the authenticated scene; a new load must reject the replacement. Windows
must deny replacement while the surviving mapping exists, then allow it after
release. This follows [ADR-0011](../adr/0011-map-content-files.md); it does not
permit in-place writes or truncation of mapped files.

## Current schema boundaries

[ADR-0010](../adr/0010-agnostic-content-packs.md) supersedes the ticket's older
caller-role and platform selectors. Tests enforce the authenticated scene type's
allowed collections instead. A filename mismatch is valid. Primitive scene
identities need not be unique and geometry suitability belongs to consumers, as
specified in [scene v1](../../schemas/scene/v1.md); duplicate _resource_ records
are invalid.

The current payload has no cross-resource references or SDK/target resource
profiles. Invalid references and incompatible SDK artifacts must be tested in
the dependent slices that introduce their schemas, as required by #22. World
creation and SDK resource publication do not exist at this boundary; no-output
rejection is evidence of withholding prepared content, not an application
startup test. No format migration, policy size caps or private signing material
are introduced.

## Verification

Run the complete repository checks using the documented native presets. To run
only this matrix against a built harness:

```sh
uv run --locked --no-sync --project tools/content_pipeline \
  python tests/integration/content_pipeline_test.py InvalidPacksTest
ctest --preset debug -R ContentFileReplacement
```

The test driver's existing `BLACKFLOWER_CONTENT_HARNESS` override also selects a
Windows executable when WSL interoperability is available. Native and target
execution results must be recorded separately.

On 2026-09-09, Linux Debug (ASan/UBSan), TSan and Release each passed the
complete `check` target, including clang-tidy and Python formatting, lint and
type checks. Windows Release cross-compilation and the complete `analyze` target
also passed, including the Windows pathname-test branch. After the review
correction, `ctest --preset <preset>` again passed all ten CTest entries per
preset. The Python suite has thirteen test methods, eleven in the invalid-pack
matrix, with subcases for individual mutations and truncated prefixes. No
sanitizer diagnostics occurred.

Execution used the development container on kernel
`6.18.33.2-microsoft-standard-WSL2`, Clang 21.1.8 and Python 3.14.4, with the
repository's locked dependencies. These are local incremental builds, not the
fresh offline DEV-Q01 experiment. Windows execution is unavailable in this
container: WSL executable/path interoperability is not exposed. The new Windows
pathname branch therefore remains an explicit execution limitation.

Separate Standards and Spec reviews found no remaining issues. The Spec review
identified that truncated light/spawn controls must use their permitted scene
types; corrected fixtures now first prove complete-record acceptance, then
reject every truncated prefix under that same type.
