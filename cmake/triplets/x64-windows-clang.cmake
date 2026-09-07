set(VCPKG_TARGET_ARCHITECTURE x64)
set(VCPKG_CRT_LINKAGE dynamic)
set(VCPKG_LIBRARY_LINKAGE static)
set(VCPKG_CHAINLOAD_TOOLCHAIN_FILE
    "${CMAKE_CURRENT_LIST_DIR}/../toolchains/clang-windows.cmake")
set(VCPKG_HASH_ADDITIONAL_FILES
    "$ENV{XWIN_ROOT}/crt/include/yvals_core.h"
    "$ENV{XWIN_ROOT}/sdk/include/shared/sdkddkver.h")
