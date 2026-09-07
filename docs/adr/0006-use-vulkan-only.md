# Use Vulkan as the only graphics backend

Status: accepted, explicitly required by the owner. Falcor integration and cross-compiled Windows execution remain unvalidated.

Blackflower uses Vulkan exclusively for graphics. DirectX 12 is not a supported build, execution, or fallback path. The offline Python cooker uses Slang to compile shaders to SPIR-V for the client pack; the client loads those prepared artifacts through its external graphics adapter. This replaces the earlier D3D12 integration proposal and fixes the graphics backend across development and deployment.

Configure or adapt Falcor and its graphics dependencies for Vulkan-only integration, excluding DirectX 12 backend code and graphics runtime dependencies from the delivered client. Verify the resulting build and artifact dependencies as well as actual Vulkan execution; a backend-selection flag alone is insufficient evidence. If Vulkan or a required capability is unavailable, startup fails explicitly. The server has no graphics backend dependency.

The selected Falcor path must consume precompiled SPIR-V and its required metadata without runtime source compilation. Exact Vulkan/SPIR-V capability profiles and SDK versions remain implementation choices requiring validation. This graphics constraint does not replace the separately required GPU PhysX integration or prohibit its compute dependencies. See the [stack](../technology-stack.md), [cooker design](../cooker-and-packs.md), and [runtime lifecycle](../runtime-lifecycle.md).
