# Blackflower xtask

`xtask` contains repository development and validation commands that do not
belong in runtime binaries. Run it from the repository root through the Cargo
alias defined in `.cargo/config.toml`:

```bash
cargo xtask --help
```

The default workspace root is the current directory; override it before the
target name when invoking the tool elsewhere:

```bash
cargo xtask --workspace-root /path/to/blackflower assets check
```

The workspace root selects the asset source tree and cooking output. It also
anchors relative `keys generate --output` and `network-gate --output` paths.
Explicit asset package and key paths are interpreted from the process working
directory.

## Targets

### `assets check`

Validate cooking profiles, asset and map manifests, package composition, and
referenced glTF or GLB sources without producing a package:

```bash
cargo xtask assets check
```

### `assets cook`

Cook and sign one package. `--profile` defaults to `debug`, `--package`
defaults to `pak000`, and `--signing-key` is a required Ed25519 private key in
PKCS#8 PEM format:

```bash
cargo xtask assets cook \
  --profile debug \
  --package pak000 \
  --signing-key .local-network/asset-signing-key.pem
```

The package is written below `target/assets/packages/<PROFILE>/`. Cooking is
deterministic for the same sources, profile, package composition, and signing
key.

### `assets verify`

Authenticate every package in a directory and validate its contents before
printing the resulting asset-set hash:

```bash
cargo xtask assets verify \
  --dir target/assets/packages/debug \
  --trusted-key .local-network/asset-signing-public.pem
```

`--trusted-key` accepts an Ed25519 public key in SPKI PEM format and can be
repeated during key rotation. `--expected-set-hash <64_HEX>` additionally
requires an exact asset-set identity:

```bash
cargo xtask assets verify \
  --dir target/assets/packages/debug \
  --trusted-key .local-network/asset-signing-public.pem \
  --expected-set-hash <64_HEX>
```

### `assets inspect`

Resolve one logical asset through the package override stack and print the
winning package, content hash, signer, and every package containing a candidate:

```bash
cargo xtask assets inspect \
  --dir target/assets/packages/debug \
  --trusted-key .local-network/asset-signing-public.pem \
  <ASSET_ID>
```

As with `verify`, `--trusted-key` can be repeated. Inspection authenticates the
package set before resolving the asset.

### `keys generate`

Use the `openssl` executable from `PATH` to generate the complete local
development credential fixture:

```bash
cargo xtask keys generate
```

The command defaults to the `.local-network` directory and a TLS server name of
`localhost`. Both can be changed:

```bash
cargo xtask keys generate \
  --output .local-staging-network \
  --server-name staging.internal.example
```

The output contains:

| File | Purpose |
| --- | --- |
| `service-ca-key.pem` | Private local service-CA key; keep offline from clients and servers |
| `service-ca.pem` | Service-CA certificate trusted by the client |
| `server-key.pem` | Private TLS key used by the server |
| `server-leaf.pem` | Leaf TLS certificate for the selected DNS name |
| `server-chain.pem` | Leaf and CA certificates supplied by the server |
| `asset-signing-key.pem` | Private PKCS#8 Ed25519 key used only by the cooker |
| `asset-signing-public.pem` | Public SPKI Ed25519 key trusted by clients and servers |

TLS certificates are short-lived local fixtures valid for one day. Generation
occurs in a temporary directory, all results are validated with OpenSSL, and
the complete directory is published only after every step succeeds. The target
refuses to replace an existing output directory. On Unix, the directory is
mode `0700` and private keys are mode `0600`.

This target is for local development and tests. Production TLS and asset-signing
keys must come from the deployment's certificate and secret-management systems.

### `network-gate`

Run the deterministic, paced network impairment simulation for 32 clients and
emit a schema-1 JSON report. The default `smoke` profile runs for five seconds:

```bash
cargo xtask network-gate
```

Available profiles are:

| Profile | Duration | Maximum p99 RTT | Maximum jitter | Maximum loss |
| --- | ---: | ---: | ---: | ---: |
| `smoke` | 5 seconds | 100 ms | 10 ms | 1% |
| `nominal` | 30 minutes | 100 ms | 10 ms | 1% |
| `degraded` | 10 minutes | 180 ms | 30 ms | 5% |

Select a profile, deterministic seed, and report file with:

```bash
cargo xtask network-gate \
  --profile degraded \
  --seed 4502437209 \
  --output target/network-gate-degraded.json
```

Without `--output`, the JSON report is written to standard output. A supplied
output path is relative to the workspace root. The command returns a failure if
the measured report exceeds the selected profile's thresholds.

## Local authenticated asset example

Generate local credentials, validate the source tree, cook the default package,
and verify the published package set:

```bash
cargo xtask keys generate
cargo xtask assets check
cargo xtask assets cook \
  --profile debug \
  --package pak000 \
  --signing-key .local-network/asset-signing-key.pem
cargo xtask assets verify \
  --dir target/assets/packages/debug \
  --trusted-key .local-network/asset-signing-public.pem
```

Keep `asset-signing-key.pem`, `service-ca-key.pem`, and `server-key.pem` out of
source control. The repository ignores the default `.local-network` directory.
