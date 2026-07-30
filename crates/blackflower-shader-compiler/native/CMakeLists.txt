cmake_minimum_required(VERSION 3.22 FATAL_ERROR)

project(BlackflowerShaderCompilerBindings LANGUAGES C CXX)

set(CMAKE_DISABLE_FIND_PACKAGE_Git ON CACHE BOOL "" FORCE)
set(SLANG_VERSION_FULL "2026.14.1" CACHE STRING "" FORCE)
set(SLANG_VERSION_NUMERIC "2026.14.1" CACHE STRING "" FORCE)
set(SLANG_LIB_TYPE STATIC CACHE STRING "" FORCE)
set(SLANG_ENABLE_DXIL OFF CACHE BOOL "" FORCE)
set(SLANG_ENABLE_GFX OFF CACHE BOOL "" FORCE)
set(SLANG_ENABLE_SLANG_RHI OFF CACHE BOOL "" FORCE)
set(SLANG_ENABLE_SLANGD OFF CACHE BOOL "" FORCE)
set(SLANG_ENABLE_SLANGC OFF CACHE BOOL "" FORCE)
set(SLANG_ENABLE_SLANGI OFF CACHE BOOL "" FORCE)
set(SLANG_ENABLE_SLANGRT OFF CACHE BOOL "" FORCE)
set(SLANG_ENABLE_SLANG_GLSLANG OFF CACHE BOOL "" FORCE)
set(SLANG_ENABLE_SLANG_PROXY OFF CACHE BOOL "" FORCE)
set(SLANG_ENABLE_TESTS OFF CACHE BOOL "" FORCE)
set(SLANG_ENABLE_EXAMPLES OFF CACHE BOOL "" FORCE)
set(SLANG_ENABLE_REPLAYER OFF CACHE BOOL "" FORCE)
set(SLANG_ENABLE_MIMALLOC OFF CACHE BOOL "" FORCE)
set(SLANG_ENABLE_SPIRV_TOOLS_MIMALLOC OFF CACHE BOOL "" FORCE)
set(SLANG_ENABLE_CUDA OFF CACHE BOOL "" FORCE)
set(SLANG_ENABLE_OPTIX OFF CACHE BOOL "" FORCE)
set(SLANG_ENABLE_XLIB OFF CACHE BOOL "" FORCE)
set(SLANG_ENABLE_AFTERMATH OFF CACHE BOOL "" FORCE)
set(SLANG_ENABLE_NVAPI OFF CACHE BOOL "" FORCE)
set(SLANG_ENABLE_PCH OFF CACHE BOOL "" FORCE)
set(SLANG_ENABLE_RELEASE_DEBUG_INFO OFF CACHE BOOL "" FORCE)
set(SLANG_ENABLE_SPLIT_DEBUG_INFO OFF CACHE BOOL "" FORCE)
set(SLANG_STANDARD_MODULE_DEVELOP_BUILD OFF CACHE BOOL "" FORCE)
set(SLANG_SLANG_LLVM_FLAVOR DISABLE CACHE STRING "" FORCE)

add_subdirectory(
    "${CMAKE_CURRENT_LIST_DIR}/../vendor/slang"
    "${CMAKE_CURRENT_BINARY_DIR}/slang"
)

add_library(blackflower_shader_compiler_wrapper STATIC wrapper.cpp)
target_compile_features(blackflower_shader_compiler_wrapper PRIVATE cxx_std_17)
target_compile_definitions(
    blackflower_shader_compiler_wrapper
    PRIVATE BF_SLANG_VERSION="2026.14.1"
)
target_include_directories(
    blackflower_shader_compiler_wrapper
    PRIVATE
    "${CMAKE_CURRENT_LIST_DIR}/../vendor/slang/include"
)
target_link_libraries(
    blackflower_shader_compiler_wrapper
    PRIVATE slang
)

set(BLACKFLOWER_INSTALL_LIB "${CMAKE_INSTALL_PREFIX}/blackflower-slang-lib")
add_custom_target(
    blackflower_shader_compiler_install
    COMMAND
        "${CMAKE_COMMAND}" -E make_directory
        "${BLACKFLOWER_INSTALL_LIB}"
    COMMAND
        "${CMAKE_COMMAND}" -E copy_if_different
        "$<TARGET_FILE:blackflower_shader_compiler_wrapper>"
        "${BLACKFLOWER_INSTALL_LIB}"
    COMMAND
        "${CMAKE_COMMAND}" -E copy_if_different
        "$<TARGET_FILE:slang>"
        "${BLACKFLOWER_INSTALL_LIB}/${CMAKE_STATIC_LIBRARY_PREFIX}blackflower_slang_compiler${CMAKE_STATIC_LIBRARY_SUFFIX}"
    COMMAND
        "${CMAKE_COMMAND}" -E copy_if_different
        "$<TARGET_FILE:compiler-core>"
        "${BLACKFLOWER_INSTALL_LIB}/${CMAKE_STATIC_LIBRARY_PREFIX}blackflower_slang_compiler_core${CMAKE_STATIC_LIBRARY_SUFFIX}"
    COMMAND
        "${CMAKE_COMMAND}" -E copy_if_different
        "$<TARGET_FILE:core>"
        "${BLACKFLOWER_INSTALL_LIB}/${CMAKE_STATIC_LIBRARY_PREFIX}blackflower_slang_core${CMAKE_STATIC_LIBRARY_SUFFIX}"
    COMMAND
        "${CMAKE_COMMAND}" -E copy_if_different
        "$<TARGET_FILE:miniz>"
        "${BLACKFLOWER_INSTALL_LIB}/${CMAKE_STATIC_LIBRARY_PREFIX}blackflower_slang_miniz${CMAKE_STATIC_LIBRARY_SUFFIX}"
    COMMAND
        "${CMAKE_COMMAND}" -E copy_if_different
        "$<TARGET_FILE:lz4_static>"
        "${BLACKFLOWER_INSTALL_LIB}/${CMAKE_STATIC_LIBRARY_PREFIX}blackflower_slang_lz4${CMAKE_STATIC_LIBRARY_SUFFIX}"
    COMMAND
        "${CMAKE_COMMAND}" -E copy_if_different
        "$<TARGET_FILE:libcmark-gfm>"
        "${BLACKFLOWER_INSTALL_LIB}/${CMAKE_STATIC_LIBRARY_PREFIX}blackflower_slang_cmark_gfm${CMAKE_STATIC_LIBRARY_SUFFIX}"
    DEPENDS
        blackflower_shader_compiler_wrapper
        slang
        compiler-core
        core
        miniz
        lz4_static
        libcmark-gfm
    VERBATIM
)
