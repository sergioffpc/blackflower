# Pin the C++ and Python development userspace

Status: accepted for [#33](https://github.com/sergioffpc/blackflower/issues/33).
Local container startup and offline native checks passed; see the
[validation record](../validation/development-container.md).

The owner selected a WSL Dev Container after VS Code selected the host Python
without the cooker's OpenUSD dependency. Supply the C++ toolchain, standard
library, Python interpreter, and validation tools in one digest-pinned Ubuntu
image with a complete system-package lock containing exact URLs and SHA-256
digests. Retain vcpkg and uv project dependency locks and use the same image
recipe for native build CI.

This preserves the existing Ubuntu/Clang and editor integration while removing
installed host tools from normal compilation. A host virtual environment alone
does not isolate the C++ compiler or the Python installation it references. A
Nix-based environment would add another package/build integration alongside
vcpkg; the container retains the existing toolchain.

Keep sources and persistent generated volumes on Linux storage under WSL.
Prepare dependencies online and then validate fresh build directories with
networking disabled. Kernel/hardware behavior, editor extensions, CodeQL
extraction, Windows SDK provisioning, and bit-for-bit output reproducibility
remain separate boundaries. See the
[container guide](../development-container.md) for operation and evidence
requirements.
