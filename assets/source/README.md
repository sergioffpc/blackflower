# Asset sources

The cooker recursively discovers files named `asset.toml` below this
directory. Other files are inputs referenced by those manifests.

Stage 0 supports opaque blobs:

```toml
schema = 1
id = "fixtures/example"
kind = "blob"
audience = "shared"
dependencies = []

[blob]
source = "example.bin"
```

Package composition has one canonical location:

```text
packages/<logical-name>/package.toml
```

For example, `--package pak000` reads `packages/pak000/package.toml`:

```toml
schema = 1
roots = ["fixtures/example"]
```

The cooker includes those roots and their transitive dependencies. There is no
separate level manifest or command-line composition override. IDs, package
names, dependency ordering, source containment, and schemas are validated
before a package is written.
