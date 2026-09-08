include_guard(GLOBAL)

if(NOT DEFINED ENV{XWIN_ROOT} OR "$ENV{XWIN_ROOT}" STREQUAL "")
  message(FATAL_ERROR "Set XWIN_ROOT to the prepared Windows SDK. See docs/build.md.")
endif()

set(CMAKE_SYSTEM_NAME Windows)
set(CMAKE_SYSTEM_PROCESSOR AMD64)
set(CMAKE_C_COMPILER clang-21)
set(CMAKE_CXX_COMPILER clang++-21)
set(CMAKE_C_COMPILER_TARGET x86_64-pc-windows-msvc)
set(CMAKE_CXX_COMPILER_TARGET x86_64-pc-windows-msvc)
set(CMAKE_CXX_STANDARD 23)
set(CMAKE_CXX_STANDARD_REQUIRED ON)
set(CMAKE_CXX_EXTENSIONS OFF)
set(CMAKE_CXX_SCAN_FOR_MODULES OFF)

# Upstream LLVM ASan cannot intercept the Microsoft Debug CRT consistently.
# Use the dynamic release CRT for project code and dependencies in every config;
# Debug still retains its unoptimized code, symbols, assertions, and ASan.
set(CMAKE_MSVC_RUNTIME_LIBRARY "MultiThreadedDLL" CACHE STRING
    "Windows runtime shared by project code and dependencies" FORCE)

find_program(CMAKE_LINKER NAMES lld-link-21 REQUIRED)
find_program(BLACKFLOWER_LLVM_AR NAMES llvm-ar-21 REQUIRED)
# GNU-style Clang archive rules use ar flags, not llvm-lib flags.
set(CMAKE_AR "${BLACKFLOWER_LLVM_AR}" CACHE FILEPATH "Target archiver" FORCE)
find_program(CMAKE_RC_COMPILER NAMES llvm-rc-21 REQUIRED)
find_program(CMAKE_MT NAMES llvm-mt-21 REQUIRED)
find_program(SCCACHE_EXECUTABLE NAMES sccache REQUIRED)
set(CMAKE_C_COMPILER_LAUNCHER "${SCCACHE_EXECUTABLE}")
set(CMAKE_CXX_COMPILER_LAUNCHER "${SCCACHE_EXECUTABLE}")

foreach(language C CXX)
  # Clang's intrinsic definitions must precede the MSVC declarations.
  execute_process(COMMAND "${CMAKE_${language}_COMPILER}" -print-resource-dir
    OUTPUT_VARIABLE clang_resource_dir OUTPUT_STRIP_TRAILING_WHITESPACE
    COMMAND_ERROR_IS_FATAL ANY)
  string(APPEND CMAKE_${language}_FLAGS_INIT
         " -isystem \"${clang_resource_dir}/include\"")
  foreach(include_dir crt/include sdk/include/ucrt sdk/include/um sdk/include/shared)
    string(APPEND CMAKE_${language}_FLAGS_INIT
           " -isystem \"$ENV{XWIN_ROOT}/${include_dir}\"")
  endforeach()
endforeach()

foreach(kind EXE SHARED MODULE)
  foreach(lib_dir crt/lib/x86_64 sdk/lib/ucrt/x86_64 sdk/lib/um/x86_64)
    string(APPEND CMAKE_${kind}_LINKER_FLAGS_INIT
           " -Xlinker /libpath:\"$ENV{XWIN_ROOT}/${lib_dir}\"")
  endforeach()
endforeach()
