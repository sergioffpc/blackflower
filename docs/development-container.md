# C++ and Python development container

The owner selected a WSL Dev Container for
[#33](https://github.com/sergioffpc/blackflower/issues/33). The definition in
[.devcontainer](../.devcontainer/devcontainer.json) supplies the Linux x86-64
toolchain, Python interpreter, and source-check tools used by VS Code and native
build-validation CI. Local startup and fresh offline Debug, TSan, and Release
checks passed; the [validation record](validation/development-container.md)
documents the environment, results, and remaining boundaries.

## Start in WSL

Provide a Docker Engine reachable from the WSL distribution, or Docker Desktop
with its WSL 2 backend and integration enabled for that distribution. Install
the VS Code Dev Containers extension on Windows. Verify `docker version`
succeeds in WSL. When installing Docker Engine locally, add the development user
to the `docker` group and start a fresh WSL session so the editor receives the
new group membership.

Keep the checkout on the Linux filesystem, such as `~/src/blackflower`. Complete
the [Git LFS setup](git-workflow.md#starting-work) in WSL so the reference
fixtures contain their binary contents before opening the container. Open the
checkout with `code .`. Run **Dev Containers: Reopen in Container**. The first
image build installs tools; the creation hook installs locked Python and
JavaScript packages and configures C++ Debug, including vcpkg dependencies. It
also verifies the OpenUSD import.

Use an ordinary clone for the validation commands below. Git worktrees need
their shared Git metadata mounted too; a bind mount of the worktree alone cannot
resolve its external `.git` pointer.

VS Code selects `tools/cooker/.venv/bin/python`, clangd 21, and the Debug
compilation database. CMake tasks and debugger configurations remain those in
the [editor guide](editor.md). The container grants `SYS_PTRACE` for debugging
and disables Docker's seccomp filter so ThreadSanitizer can adjust its process
address layout on WSL. This development environment is for trusted project code;
it is not a sandbox for executing untrusted programs.

Signing requires the developer's forwarded SSH/GPG agent and Git identity;
private keys are not included in the image. For SSH signing, host key-file paths
are unavailable inside the container: configure public key text with a forwarded
agent, or a public key file accessible there. Keep signing enabled and verify a
signed commit under the [Git workflow](git-workflow.md).

From the integrated terminal:

```sh
cmake --build --preset debug --target check
bash .devcontainer/prepare.sh tsan release
cmake --build --preset tsan --target check
cmake --build --preset release --target check
```

## Fixed inputs and isolation

The [Dockerfile](../.devcontainer/Dockerfile) fixes the Ubuntu image by its
Linux amd64 manifest digest. The
[system-package lock](../.devcontainer/system-packages.lock) fixes 179
additional Debian packages by URL and SHA-256, including LLVM 21, libstdc++,
libc, CMake, Ninja, sccache, and Python. This closure was resolved against
authenticated Ubuntu indexes on 2026-09-08. Image construction verifies these
exact files and installs them locally after removing live APT sources. It never
resolves package versions from a current package index. Missing files and hash
mismatches fail construction.

Downloaded uv 0.10.4 and Node 22.22.1 archives also have fixed SHA-256 digests.
Source style binaries retain the existing checksum-pinned installer. The image's
`/opt/blackflower/system-packages.tsv` records all installed system versions.

The cooker selects Python 3.14.4 through its version file and installs packages
from `uv.lock` with `--locked`. Container settings prohibit interpreter
downloads; Python and its standard library come from the fixed image. The vcpkg
checkout comes from the manifest baseline; preparation rejects a mismatch. Its
source hashes and overlays continue to select C++ dependencies.

Named volumes hold `build/`, the cooker `.venv`, JavaScript `node_modules`, and
compiler/package caches. They hide corresponding host directories, survive
container recreation, and are separate for each Dev Container identity. Sources
remain a bind mount. Keep host include directories, interpreters, and compilers
outside this environment.

The root startup entrypoint repairs generated-volume ownership after VS Code
remaps the Linux UID. Interactive tools run as `ubuntu`; CI validation runs as
root. Normal restarts skip recursive ownership scans when the volume owner
already matches. The writable vcpkg checkout lives under the user's home so UID
remapping covers it too.

Rebuild the image when its inputs change. Review the image digest and complete
system-package lock together. Changing toolchains requires fresh CMake build
directories, while compiler/package caches can remain available. Run preparation
after changing project lockfiles. Package repositories and upstream archives
have finite retention; preserve validated images and dependency assets for
long-term offline recovery.

To deliberately update system packages, run the maintenance resolver against the
exact base image from the Dockerfile and review the output before replacing the
lock. Image builds never invoke the resolver:

```sh
mkdir -p build
docker run --rm \
  --mount "type=bind,source=$PWD/.devcontainer,target=/config,readonly" \
  ubuntu:26.04@sha256:889d056d5c6c0bfb55789ff3710681d68e50713cb562d2196dc07110599c7a6f \
  bash /config/resolve-system.sh > build/system-packages.lock.new
```

Regenerate the closure when the base image changes. Rebuild and repeat offline
validation after accepting a lock update.

This fixes the development userspace and provides an offline build check. The
host kernel, Docker engine, CPU, memory, editor/extensions, and GPU drivers
remain external. It does not establish byte-identical binaries or GPU/audio
runtime compatibility. CodeQL extraction retains its separately documented host
setup.

## Compilation performance

Keep sources, generated volumes, and caches on Linux storage. Paths such as
`/mnt/c` introduce cross-filesystem I/O into header scanning and small-file
builds. Image construction and dependency downloads are cold setup costs,
separate from ordinary incremental compilation. Ninja and sccache continue to
work inside the container, retaining caches between sessions.

Preparation and the editor startup hook start sccache before vcpkg. The daemon
stays active until the container stops. This prevents it inheriting vcpkg's
filesystem locks and delaying the next configuration until its idle timeout.

No measured host/container performance ratio is established. Compare the same
source revision, toolchain/library versions, preset, CPU/memory limits, and job
count. Measure dependency preparation separately. Compare clean compilation with
compiler caching disabled, then unchanged and single-file incremental builds
with equivalent cache states. Repeat and report the configuration and timings
instead of promising identical times.

## Offline validation

Run from a disposable ordinary clone in WSL after completing the
[Git LFS setup](git-workflow.md#starting-work) for that clone. Root validation
can leave generated Python package metadata in that checkout owned by root. Keep
this clone separate from the interactive editor checkout. The container name
must be unused:

```sh
docker build -f .devcontainer/Dockerfile -t blackflower-dev:local .
docker run --detach --name blackflower-offline --user root \
  --cap-add SYS_PTRACE --security-opt seccomp=unconfined \
  --mount "type=bind,source=$PWD,target=/workspaces/blackflower" \
  --mount type=volume,target=/workspaces/blackflower/build \
  --mount type=volume,target=/workspaces/blackflower/tools/cooker/.venv \
  --mount type=volume,target=/workspaces/blackflower/tools/style/node_modules \
  blackflower-dev:local
docker exec blackflower-offline bash .devcontainer/prepare.sh debug tsan release
docker network disconnect bridge blackflower-offline
docker exec blackflower-offline bash .devcontainer/check.sh debug
docker exec blackflower-offline bash .devcontainer/check.sh tsan
docker exec blackflower-offline bash .devcontainer/check.sh release
docker cp blackflower-offline:/workspaces/blackflower/build build/container-evidence
docker rm --force --volumes blackflower-offline
```

Each check creates a fresh build directory, disables compiler and vcpkg binary
cache reuse, and runs the existing analysis and CTest gate. Sources and tools
must already be downloaded. Debug also runs source style and hook tests. Retain
`check.log` and `Testing/Temporary/` with the image ID, source revision, kernel,
resource limits, and commands. Copy evidence before removing the validation
container and its anonymous generated volumes. Normal VS Code volumes are
independent.

The native [CI workflow](../.github/workflows/ci.yml) uses the same sequence for
three presets. It caches the image by its recipe inputs and preserves dependency
and compiler caches; offline checks still disable compiled-cache reuse. A host
build or Dockerfile syntax check cannot establish container correctness.

## Windows cross-compilation

The image includes LLVM's Windows linker/resource tools and 7zip. Prepare the
exact Windows SDK/CRT and Windows ASan runtime inside its build volume using the
[cross-build guide](build.md#windows-cross-build-from-linux), then export
`XWIN_ROOT` and `LLVM_WINDOWS_ASAN_DIR` in the container shell. Microsoft
license acceptance and SDK assets remain separately provisioned; native offline
tests do not certify the Windows toolchain closure. Windows binaries still
require execution on Windows for acceptance.

## References

-   [VS Code Dev Containers](https://code.visualstudio.com/docs/devcontainers/containers).
-   [WSL filesystem performance](https://docs.docker.com/desktop/features/wsl/best-practices/).
-   [Image digest pinning](https://docs.docker.com/build/building/best-practices/#pin-base-image-versions).
-   [Python selection in uv](https://docs.astral.sh/uv/concepts/python-versions/).
