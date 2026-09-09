# Agnostic content packs

Status: accepted.

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
