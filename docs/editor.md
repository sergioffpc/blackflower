# Reference editor: Visual Studio Code

Visual Studio Code is the reference editor on Ubuntu 26.04 x86-64. The workspace
configuration uses the same CMake presets, Clang toolchain, vcpkg manifest, and
checks as the command line and CI.

## Setup

Complete the [build prerequisites and vcpkg setup](build.md#prerequisites), then
install clangd 21:

```sh
sudo apt-get install clangd-21
```

Launch VS Code from a shell with the build tools on PATH and VCPKG_ROOT
exported:

```sh
code .
```

VS Code inherits its environment when its process starts. Fully quit existing
instances before launching it from a newly prepared shell if tool paths or
VCPKG_ROOT have changed. Set machine-specific paths in your environment or user
settings; keep shared workspace files portable.

Install the workspace's recommended extensions when prompted, or use
**Extensions: Show Recommended Extensions**:

| Extension             | Responsibility                                                                        |
| --------------------- | ------------------------------------------------------------------------------------- |
| Microsoft CMake Tools | Preset selection, builds, and CTest discovery in the Testing view.                    |
| LLVM clangd           | Completion, navigation, diagnostics, and formatting using .clang-format.              |
| CodeLLDB              | Debugging the executable and GoogleTest harness. Its extension includes the debugger. |

The recommendations are declared in
[.vscode/extensions.json](../.vscode/extensions.json).
[.vscode/settings.json](../.vscode/settings.json) selects clangd-21 and disables
the Microsoft C/C++ IntelliSense engine if that separate extension is installed,
preventing duplicate language diagnostics.

## Configure and build

1.  Run **CMake: Select Configure Preset** and choose debug.
2.  Run **CMake: Configure**, then **CMake: Select Build Preset** and choose
    debug.
3.  Run **CMake: Build**. The first configuration installs the pinned vcpkg
    dependencies and requires network access.

CMake Tools copies the selected build's compilation database to the ignored root
compile_commands.json. clangd discovers that file for the actual C++23 flags and
dependency includes. Reconfigure through CMake Tools after switching presets;
restart clangd if diagnostics retain an old configuration. A terminal-only
configuration can be exposed to clangd with
`cp build/debug/compile_commands.json compile_commands.json`.

**Ctrl+Shift+B** runs the supplied Build Debug task, including configuration.
**Tasks: Run Task → Check Debug** runs the complete Debug acceptance checks.
These tasks deliberately use debug regardless of CMake Tools' selected preset.
To build or check Release or ThreadSanitizer, use the release or tsan preset
through CMake Tools or the [command-line instructions](build.md#verification).

## Tests, benchmarks, and debugging

After building, select the matching CMake test preset and refresh the Testing
view to discover CTest entries, including individual GoogleTest cases. Run
**CMake: Run Tests** or use the Testing view. For the full analysis and
formatting gate, run Check Debug.

Set a breakpoint in src/main.cc, choose **Debug Blackflower** in Run and Debug,
and press **F5**. Choose **Debug GoogleTest** to debug the test executable; its
launch arguments can include a GoogleTest filter when investigating one test.
Both launch configurations build Debug first and use CodeLLDB. Their environment
disables LeakSanitizer while debugging because it cannot run under ptrace; ASan
memory checks remain enabled, and normal CTest/CI runs retain leak detection.

Run Google Benchmark from the integrated terminal using the
[Release benchmark command](build.md#benchmarks). The current benchmark only
checks the framework setup; meaningful performance comparisons require a product
workload and controlled hardware.

Formatting on save uses clangd's Google-based formatter. Editor diagnostics are
development feedback; the CMake check target remains the acceptance gate.

## References

-   [CMake Tools on Linux](https://code.visualstudio.com/docs/cpp/cmake-linux).
-   [CMake Tools settings](https://github.com/microsoft/vscode-cmake-tools/blob/main/docs/cmake-settings.md).
-   [clangd editor setup](https://clangd.llvm.org/installation).
-   [CodeLLDB manual](https://github.com/vadimcn/codelldb/blob/master/MANUAL.md).
