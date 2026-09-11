# Agnostic content packs

Status: accepted; the entity contract amendment below supersedes the original
collection and unit decisions.

The cooker derives complete ServerScene, AgentScene and ClientScene artifacts
from one source scene. Applications select their own file; resource types and
schemas define compatibility. Each file type has its own authenticated magic,
which selects the concrete scene alternative returned by the loader. The loading
API requires no caller role or platform selector.

ServerScene and AgentScene share collision geometry without visual or audio
assets. Spawn points belong only to ServerScene. ClientScene contains collision
geometry and supported presentation content, without spawn points. Geometry uses
millimetres; runtime adapters create SDK resources from the portable
definitions.

[Pack v1](../../schemas/pack/v1.md) defines `.bfserver`, `.bfagent` and
`.bfclient` publication and their common content build identity. Autonomous
participants will connect and send inputs through the client protocol. Only
their content is prepared here; their runtime and model-driven control are not
defined.

This supersedes the pack selection and format decisions in
[ADR-0005](0005-require-signed-cooked-content.md) and
[ADR-0007](0007-minimal-pack-format-and-trust.md). Independent trust, offline
cooking and validation remain required. Schema evolution follows the
[release policy](../development-process.md#schema-versioning).

## Entity contract amendment

On 2026-09-10, the owner refined the first entity slice in
[#58](https://github.com/sergioffpc/blackflower/issues/58): source USD and all
three internal scenes contain only entities, each with a persistent string ID,
placement and optional independently authored `Bounds`. This supersedes the
global collision/light/spawn collections described above. Visual resources
remain a subsequent entity-owned representation.

The [Scene v1 contract](../../schemas/scene/v1.md) now stores binary64 metre
coordinates and unit XYZW quaternions. This replaces integer millimetres to
preserve rotated boxes and composed transforms without the previous quantization
step. In #58, bounds were stored in world space inside their owning entity.
[ADR-0014](0014-instantiate-local-scene-entity-descriptions-in-ecs.md)
supersedes that coordinate choice with local Collider descriptions and
exactly-once placement in #61. Consumers no longer receive global spawn or light
collections.

The bounded [authoring contract](../usd-authoring.md) permits one relative
external entity definition per placement. It supersedes ADR-0007's single-file
source restriction: exact bytes are captured before composition and a canonical
dependency transcript determines source provenance. This enables reusable
definitions and relocation while retaining complete input coverage. Definitions
cannot reference further layers; authors must keep source files unchanged
throughout capture.

Portable role selection, independent trust and atomic pack-set publication
remain unchanged. Development schemas evolve in place, requiring regeneration of
older packs rather than a compatibility path.

## Sparse role-scene amendment

On 2026-09-11, the owner approved role-specific entity domains in
[#76](https://github.com/sergioffpc/blackflower/issues/76), implemented for the
content boundary by [#78](https://github.com/sergioffpc/blackflower/issues/78).
The cooker now projects one source catalogue into sparse ServerScene, AgentScene
and ClientScene payloads before deriving their common `ContentBuildId`.

ServerScene physically contains `SessionStatic` and `AuthoritativeDynamic`
collider-bearing descriptions. AgentScene contains `SessionStatic` collision
only. Both omit presentation references. ClientScene contains one union
catalogue with the `SessionStatic` collision, logical visual and logical audio
domains needed by later Prediction and Presentation projection; it physically
omits `AuthoritativeDynamic` collider data. An entity absent from a role has no
placeholder record; an entity retained by more than one role or client domain
keeps its authored `SceneEntityId`.

The authenticated magic remains sufficient to decode a pack. The C++ loader also
accepts an optional expected role and returns a typed error after complete
authentication, integrity verification and scene decoding when the role is
different. This narrows the original no-selector decision: content inspection
needs no caller role, while application startup can reject a valid but misrouted
artifact at the loading boundary.

### Alternatives considered

Retaining three copies of one generic payload would leave role separation to
caller convention. Serializing independent Prediction and Presentation client
catalogues would duplicate one authored identity namespace. Treating filenames
as role authority would bypass authenticated role selection. Requiring a caller
role for every inspection would prevent generic verified-content tooling. These
alternatives were rejected in favor of authenticated variants, one sparse client
union, and an optional expected-role check.

### Consequences

Logical visual and audio references prove domain separation without choosing a
media format or creating SDK resources. Classification of authored collision is
now required; `AuthoritativeDynamic` geometry is available to ServerScene but
filtered from AgentScene and ClientScene. Dynamic behavior and lifecycle,
detailed media resources, and world construction remain separate work. Schema 1
evolves in place and older development fixtures must be regenerated.
