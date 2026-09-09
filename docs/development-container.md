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
succeeds in WSL. The terminal validation commands below also require `jq` on the
WSL host. When installing Docker Engine locally, add the development user to the
`docker` group and start a fresh WSL session so the editor receives the new
group membership.

Keep the checkout on the Linux filesystem, such as `~/src/blackflower`. Complete
the [Git LFS setup](git-workflow.md#starting-work) in WSL so the reference
fixtures contain their binary contents before opening the container. Open the
checkout with `code .`. Run **Dev Containers: Reopen in Container**. The first
image pull retrieves the committed GHCR digest; the creation hook installs
locked Python and JavaScript packages and configures C++ Debug, including vcpkg
dependencies. It also verifies the OpenUSD import.

Use an ordinary clone for the validation commands below. Git worktrees need
their shared Git metadata mounted too; a bind mount of the worktree alone cannot
resolve its external `.git` pointer.

VS Code selects `tools/content_pipeline/.venv/bin/python`, clangd 21, and the
Debug compilation database. Shared Python and editor settings live in
[workspace settings](../.vscode/settings.json), which VS Code also reads inside
the container. The Dev Container lists extensions for automatic installation;
[workspace recommendations](../.vscode/extensions.json) contain the same list.
CMake tasks and debugger configurations remain those in the
[editor guide](editor.md). The container grants `SYS_PTRACE` for debugging and
disables Docker's seccomp filter so ThreadSanitizer can adjust its process
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

## Forward the WSL SSH agent

For the Windows PowerShell → WSL → `code .` → Dev Container workflow, load the
SSH key in WSL and expose its agent socket to the editor's startup environment.
Run the following commands in the **WSL host terminal**, outside the container.
If `ssh-add -l` already lists the intended key, reuse that agent and skip
starting another one:

```bash
eval "$(ssh-agent -s)"
ssh-add
ssh-add -l
```

For a key with a custom filename, use `ssh-add ~/.ssh/name_of_private_key`
instead of plain `ssh-add`; do not select the `.pub` file. Once the key is
listed, create a stable link to this agent:

If `SSH_AUTH_SOCK` already equals `$HOME/.ssh/vscode-agent.sock` and lists the
key, keep the existing link and skip the `ln` command to avoid a self-reference.

```bash
mkdir -p ~/.ssh ~/.vscode-server
ln -sfn "$SSH_AUTH_SOCK" ~/.ssh/vscode-agent.sock
touch ~/.vscode-server/server-env-setup
```

Add this line once to both `~/.bashrc` and `~/.vscode-server/server-env-setup`
in WSL, preserving their existing content:

```sh
export SSH_AUTH_SOCK="$HOME/.ssh/vscode-agent.sock"
```

The server environment file configures VS Code's WSL server; `.bashrc` also
exposes the socket to interactive Bash startup used when probing the host
environment. In the reported failure, setting only the server environment file
was insufficient; forwarding worked after adding the export to `.bashrc`. Verify
a fresh interactive login shell without inheriting the current variable:

```bash
env -u SSH_AUTH_SOCK bash -lic \
  'printf "SSH_AUTH_SOCK=%s\n" "$SSH_AUTH_SOCK"; ssh-add -l'
```

This must list the intended key. Close all VS Code windows, reopen the checkout
with `code .` from WSL, and select **Dev Containers: Reopen in Container**. In a
new **container terminal**, verify:

```bash
printf 'SSH_AUTH_SOCK=%s\n' "$SSH_AUTH_SOCK"
ssh-add -l
```

Acceptance is a nonempty socket path and the same key fingerprint listed in WSL.
The owner confirmed this result on 2026-09-09. This verifies agent forwarding;
it does not by itself verify GitHub access or commit signing.

### Recover after restarting WSL

A WSL shutdown stops its agent. Start a new agent and load the key using the
commands above, then refresh the link with `ln -sfn` before reopening VS Code.
Refresh it whenever the agent socket changes. The shell and server environment
exports remain configured; the link alone does not start an agent or load keys.
Avoid starting another agent when an existing one already lists the key.

### Diagnose missing forwarding

If the container reports
`Could not open a connection to your authentication agent.`, inspect **Dev
Containers: Show Container Log** for `ssh-agent:` lines.
`SSH_AUTH_SOCK not set on wsl host` means the helper did not receive the WSL
variable, even if `ssh-add -l` works in the original WSL terminal. Repeat the
fresh-shell check above and restart the editor after changing startup files.

If a socket reports `Connection refused`, check the WSL link and agent:

```bash
ls -l ~/.ssh/vscode-agent.sock
SSH_AUTH_SOCK="$HOME/.ssh/vscode-agent.sock" ssh-add -l
```

Refresh the link after starting a replacement agent. Starting `ssh-agent` inside
the container creates a separate agent; `The agent has no identities` then means
that agent is reachable but empty. It does not restore forwarding from WSL. Keep
the private key on the host.

See the VS Code documentation for
[sharing SSH credentials](https://code.visualstudio.com/remote/advancedcontainers/sharing-git-credentials)
and the
[WSL server environment script](https://code.visualstudio.com/docs/remote/wsl#_advanced-environment-setup-script).

## Published image and updates

The `image` property in [devcontainer.json](../.devcontainer/devcontainer.json)
is the authoritative GHCR reference for both VS Code and native CI. It pins the
complete development image by SHA-256 digest. Ordinary validation pulls that
image and never falls back to Docker construction or Ubuntu package downloads.
Cache eviction therefore causes another registry pull, not dependency
resolution. Python, JavaScript and vcpkg preparation still need their upstream
assets before the offline checks.

The separate
[publication workflow](../.github/workflows/publish-devcontainer.yml) builds
candidates when image inputs change on `develop` or repository feature branches,
or through manual dispatch once the workflow is on the default branch. It never
runs on a pull-request event. Only its publication job has `packages: write`;
consumer CI has `packages: read`. The workflow links the package to this
repository and records its source revision and the fingerprint from
[image-inputs.sh](../.devcontainer/image-inputs.sh). CI rejects a digest whose
image inputs differ from the checkout. Keep this input list and the publication
path filters current when adding Dockerfile inputs.

GHCR initially creates packages as private. Repository workflows authenticate
with `GITHUB_TOKEN`. For a private package, developers need package read access
and a classic personal access token with `read:packages`; run
`docker login ghcr.io --username YOUR_GITHUB_LOGIN` in WSL and supply the token
at the password prompt before opening the container. Keep credentials in the
host's credential store. Public GHCR packages allow anonymous pulls. Package
visibility is a separate GitHub setting; fork workflows and contributors without
package access require a public package. See
[GHCR authentication](https://docs.github.com/en/packages/working-with-a-github-packages-registry/working-with-the-container-registry)
and
[package access](https://docs.github.com/en/packages/learn-github-packages/configuring-a-packages-access-control-and-visibility).

To update the environment:

-   Change the Dockerfile or locked inputs on a feature branch and push it. The
    snapshot bootstrap still verifies the existing package hashes.

-   Wait for **Publish development container** to succeed. Read its summary or
    download `devcontainer-image.txt` from the `devcontainer-image` artifact.
    The source-commit tag identifies a candidate; consumers never use that
    mutable tag.

-   Replace the `image` value with the published digest in the same PR as the
    input changes. Run the full source style check and push the signed commit.

-   Require the existing Debug, TSan, Release and CodeQL checks before merge.
    These validate the exact candidate against the proposed source. Publication
    alone does not establish native build acceptance.

-   Reopen or rebuild the VS Code container to use the new pin. This pulls the
    published image; fresh CMake directories avoid mixing toolchains.

Retain every digest referenced by supported branches and rollback revisions.
There is no automatic GHCR image cleanup in this project. The reference artifact
expires after 90 days, but the committed digest is the lasting reference. Roll
back the image pin and its corresponding recipe/lock changes together through a
PR; the input check intentionally rejects mismatched pairs. A registry outage or
deletion can still prevent a fresh pull. Preserve a `docker image save` export
of validated images in independently managed storage when offline recovery is
required. Rebuilding a new image still depends on snapshot and tool archive
availability; this change does not create an archive of every `.deb`.

## Fixed inputs and isolation

The [Dockerfile](../.devcontainer/Dockerfile) fixes the Ubuntu image by its
Linux amd64 manifest digest. The
[system-package lock](../.devcontainer/system-packages.lock) fixes 182
additional Debian packages by URL and SHA-256, including LLVM 21, libstdc++,
libc, CMake, Ninja, sccache, and Python. This closure was resolved against
authenticated Ubuntu indexes on 2026-09-08. Image construction verifies these
exact files and installs them locally after removing live APT sources. It never
resolves package versions from a current package index. Package URLs use the
[Ubuntu Snapshot Service](https://snapshot.ubuntu.com/) at `20260908T000000Z`,
preserving the locked versions when current mirrors remove superseded files. The
committed ISRG Root X1 certificate bootstraps HTTPS verification before the base
image installs its CA bundle. The authenticated lock's SHA-256 digests remain
mandatory. A missing file, TLS error or hash mismatch fails construction. See
the [snapshot validation](validation/development-container-snapshots.md).

The lock also includes bubblewrap 0.11.1 and its libcap2 dependency for tools
that use the `bwrap` executable. Image construction verifies their hashes and
the executable version.

The lock includes GitHub CLI (`gh`) 2.46.0-4 from Ubuntu for the repository's
issue and pull-request workflow. Image construction runs `gh --version` to
verify the executable. After updating an existing container, select **Dev
Containers: Rebuild Container** to install it. Authenticate interactively with
`gh auth login` when needed; credentials are not included in the image.

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

Publish a new candidate image when its inputs change. Review the image digest
and complete system-package lock together. Changing toolchains requires fresh
CMake build directories, while compiler/package caches can remain available. Run
preparation after changing project lockfiles. Package repositories and upstream
archives have finite retention; preserve validated images and dependency assets
for long-term offline recovery.

To deliberately update system packages, run the maintenance resolver against the
exact base image from the Dockerfile, pass an explicit snapshot timestamp, and
review the output before replacing the lock. The resolver checks signed Ubuntu
indexes and emits snapshot URLs and SHA-256 digests. Image builds never invoke
the resolver:

```sh
mkdir -p build
docker run --rm \
  --mount "type=bind,source=$PWD/.devcontainer,target=/config,readonly" \
  ubuntu:26.04@sha256:889d056d5c6c0bfb55789ff3710681d68e50713cb562d2196dc07110599c7a6f \
  bash /config/resolve-system.sh 20260908T000000Z \
  > build/system-packages.lock.new
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
image=$(jq -er .image .devcontainer/devcontainer.json)
docker pull "$image"
docker run --detach --name blackflower-offline --user root \
  --cap-add SYS_PTRACE --security-opt seccomp=unconfined \
  --mount "type=bind,source=$PWD,target=/workspaces/blackflower" \
  --mount type=volume,target=/workspaces/blackflower/build \
  --mount type=volume,target=/workspaces/blackflower/tools/content_pipeline/.venv \
  --mount type=volume,target=/workspaces/blackflower/tools/code_quality/node_modules \
  "$image"
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
three presets. It pulls the digest from the editor configuration and checks the
image's input fingerprint against the checkout. It preserves dependency and
compiler caches; offline checks still disable compiled-cache reuse. A host build
or Dockerfile syntax check cannot establish container correctness.

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
