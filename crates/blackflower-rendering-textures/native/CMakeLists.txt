cmake_minimum_required(VERSION 3.22 FATAL_ERROR)

project(BlackflowerTextureBindings LANGUAGES C CXX)

if(NOT DEFINED BLACKFLOWER_KTX_VERSION)
    message(FATAL_ERROR "BLACKFLOWER_KTX_VERSION is required")
endif()

set(BUILD_SHARED_LIBS OFF CACHE BOOL "" FORCE)
set(BUILD_TESTING OFF CACHE BOOL "" FORCE)
set(KTX_FEATURE_DOC OFF CACHE BOOL "" FORCE)
set(KTX_FEATURE_ETC_UNPACK OFF CACHE BOOL "" FORCE)
set(KTX_FEATURE_GL_UPLOAD OFF CACHE BOOL "" FORCE)
set(KTX_FEATURE_JNI OFF CACHE BOOL "" FORCE)
set(KTX_FEATURE_KTX1 OFF CACHE BOOL "" FORCE)
set(KTX_FEATURE_KTX2 ON CACHE BOOL "" FORCE)
set(KTX_FEATURE_PY OFF CACHE BOOL "" FORCE)
set(KTX_FEATURE_TESTS OFF CACHE BOOL "" FORCE)
set(KTX_FEATURE_TOOLS OFF CACHE BOOL "" FORCE)
set(KTX_FEATURE_TOOLS_CTS OFF CACHE BOOL "" FORCE)
set(KTX_FEATURE_VK_UPLOAD OFF CACHE BOOL "" FORCE)
set(ASTCENC_ISA_NONE ON CACHE BOOL "" FORCE)
set(ASTCENC_ISA_AVX2 OFF CACHE BOOL "" FORCE)
set(ASTCENC_ISA_SSE41 OFF CACHE BOOL "" FORCE)
set(ASTCENC_ISA_SSE2 OFF CACHE BOOL "" FORCE)
set(BASISU_SUPPORT_OPENCL OFF CACHE BOOL "" FORCE)
set(BASISU_SUPPORT_SSE OFF CACHE BOOL "" FORCE)

add_subdirectory(
    "${CMAKE_CURRENT_LIST_DIR}/../vendor/KTX-Software"
    "${CMAKE_CURRENT_BINARY_DIR}/ktx"
)

add_library(blackflower_texture_wrapper STATIC wrapper.cpp)
target_compile_features(blackflower_texture_wrapper PRIVATE cxx_std_17)
target_compile_definitions(
    blackflower_texture_wrapper
    PRIVATE
        KHRONOS_STATIC
        KTX_FEATURE_WRITE
        BF_KTX_VERSION="${BLACKFLOWER_KTX_VERSION}"
)
target_include_directories(
    blackflower_texture_wrapper
    PRIVATE
        "${CMAKE_CURRENT_LIST_DIR}/../vendor/KTX-Software/include"
        "${CMAKE_CURRENT_LIST_DIR}/../vendor/KTX-Software/lib"
)
target_link_libraries(blackflower_texture_wrapper PRIVATE ktx)

set(BLACKFLOWER_INSTALL_LIB "${CMAKE_INSTALL_PREFIX}/blackflower-ktx-lib")
add_custom_target(
    blackflower_texture_install
    COMMAND
        "${CMAKE_COMMAND}" -E make_directory
        "${BLACKFLOWER_INSTALL_LIB}"
    COMMAND
        "${CMAKE_COMMAND}" -E copy_if_different
        "$<TARGET_FILE:blackflower_texture_wrapper>"
        "${BLACKFLOWER_INSTALL_LIB}"
    COMMAND
        "${CMAKE_COMMAND}" -E copy_if_different
        "$<TARGET_FILE:ktx>"
        "${BLACKFLOWER_INSTALL_LIB}"
    COMMAND
        "${CMAKE_COMMAND}" -E copy_if_different
        "$<TARGET_FILE:astcenc-none-static>"
        "${BLACKFLOWER_INSTALL_LIB}"
    DEPENDS
        blackflower_texture_wrapper
        ktx
        astcenc-none-static
    VERBATIM
)
