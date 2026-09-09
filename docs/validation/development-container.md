# Development container validation

Scope: [#33](https://github.com/sergioffpc/blackflower/issues/33) and DEV-Q01.
These are local execution results from 2026-09-08. The native GitHub Actions
workflow uses the same recipe and check script; its hosted execution has not
been established by these local runs.

## Environment and source

Validation used the implementation accompanying this document on
`feature/hermetic-devcontainer`, based on
`37850fe6951c060fc8a69c1c4a41be08bef91f17`. An ordinary clone held the source;
generated build, Python, and JavaScript directories were separate Docker
volumes. The final image identity reported by Docker was:

```text
sha256:7fbe9222395f76df2b12c9c3e6e5852828e6cddcf1b804ff111de9f725a8e6c9
```

The host was WSL 2 with kernel `6.18.33.2-microsoft-standard-WSL2`, Docker
29.1.3, 64 visible logical CPUs, and about 63 GiB of visible memory. Containers
had no additional CPU or memory quota; CMake and vcpkg concurrency were limited
to four jobs. The image supplied Clang 21.1.8, CMake 4.2.3, Python 3.14.4, uv
0.10.4, Node 22.22.1, and sccache 0.13.0. Its vcpkg checkout matched the
manifest baseline `9e593bb18ea69cc5095e012465dcd675a822ed0d`.

The image built successfully using the 179-entry system-package lock and the
checksum-pinned tool archives. Installed system versions are recorded in the
image at `/opt/blackflower/system-packages.tsv`.

## Container startup and offline checks

The Dev Containers CLI bundled with VS Code's extension completed the actual
`postCreateCommand` and `postStartCommand` as `ubuntu` (UID 1000). Preparation
installed locked dependencies and configured Debug. The selected interpreter,
`/workspaces/blackflower/tools/content_pipeline/.venv/bin/python`, imported
OpenUSD `(0, 26, 8)`, and the Debug compilation database existed.

Dependencies were prepared online before native validation. The final validation
container used `--network none`, `SYS_PTRACE`, and `seccomp=unconfined`. Each
`bash .devcontainer/check.sh <preset>` invocation created a fresh build
directory and disabled compiler and vcpkg binary cache reuse. Python used
`uv sync --locked --offline`.

| Preset  | Result                                                | Evidence directory under container `build/` |
| ------- | ----------------------------------------------------- | ------------------------------------------- |
| Debug   | Eight CTest entries passed.                           | `offline-debug-9vC6KX`                      |
| TSan    | Eight CTest entries passed.                           | `offline-tsan-nBh0AP`                       |
| Release | Eight CTest entries passed; benchmark JSON generated. | `offline-release-eoj8lb`                    |

The check target includes C++ formatting and clang-tidy, Python Pyink, Pylint,
mypy and integration tests, C++ contract tests, and executable/benchmark
startup. Debug additionally passed the full source style check and the
six-language staged-content hook tests. The benchmark is a framework smoke test,
not a measurement of simulation performance or container overhead.

Local logs are retained under the owner's main checkout `.cache/` as
`container-build.log`, `container-check-<preset>-final.log`, and
`devcontainer-cli-final.log`. The named build directories retain `check.log` and
CTest logs. The
[operation guide](../development-container.md#offline-validation) provides
repeatable commands; CI uploads equivalent evidence for 14 days.

## Diagnosed startup failures

The first configuration started the sccache daemon from inside vcpkg. Inspection
of `/proc/<pid>/fd` showed the daemon retaining three `vcpkg-running.lock`
descriptors after CMake exited. A second preset configuration timed out after
five seconds with repeated lock waits; unconstrained runs waited about 600
seconds. Starting sccache before vcpkg and disabling its idle shutdown removed
the inherited descriptors. Consecutive cached configurations then completed in
1.1 and 0.8 seconds. Preparation, offline checks, and the editor startup hook
now start the daemon before configuration. These timings diagnose this lock
failure; they do not compare host and container compilation.

TSan initially failed during GoogleTest discovery because Docker's default
seccomp filter blocked the address-layout adjustment needed on this WSL kernel.
With the documented development-only seccomp setting, the same fresh TSan build
passed all eight tests. No sanitizer diagnostic was suppressed and no host-wide
ASLR setting was changed.

Running root CI-style validation and interactive development against the same
clone also exposed root-owned editable Python metadata. Correcting those test
artifacts allowed nonroot preparation to complete. The guide now uses a
disposable clone for root validation; normal editor preparation runs as the
development user.

Separate Standards and Spec reviews found no remaining implementation findings
after the Git trust and volume ownership corrections. Hosted CI results, Windows
SDK/runtime validation, GPU/audio compatibility, signed commits through the
editor's forwarded agent, and a controlled host/container compilation comparison
remain outside these local results. Long-term offline recovery still requires
retaining the image and downloaded assets.

## GitHub CLI package validation

On 2026-09-09, the image recipe added Ubuntu's `gh` 2.46.0-4 package to the
system-package lock and maintenance resolver. The downloaded amd64 package
matched the SHA-256 published on the
[Ubuntu download page](https://packages.ubuntu.com/resolute/amd64/gh/download).
Its only declared dependency is `libc6 (>= 2.34)`, already satisfied by the
locked system packages. Extracting the package and running its executable on
Ubuntu 26.04 reported `gh version 2.46.0 (2025-12-13 Ubuntu 2.46.0-4)`.

The image installation now runs `gh --version`. Docker is unavailable in the
editing environment, so a full image rebuild and offline checks for this
addition remain pending; the earlier image results above do not cover it.
