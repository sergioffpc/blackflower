cmake_minimum_required(VERSION 3.22 FATAL_ERROR)

project(BlackflowerNativeKtx LANGUAGES C CXX)

if(NOT DEFINED BLACKFLOWER_KTX_ROOT)
    message(FATAL_ERROR "BLACKFLOWER_KTX_ROOT is required")
endif()

set(BUILD_SHARED_LIBS OFF CACHE BOOL "" FORCE)
set(BUILD_TESTING OFF CACHE BOOL "" FORCE)
set(KTX_FEATURE_DOC OFF CACHE BOOL "" FORCE)
set(KTX_FEATURE_ETC_UNPACK OFF CACHE BOOL "" FORCE)
set(KTX_FEATURE_GL_UPLOAD OFF CACHE BOOL "" FORCE)
set(KTX_FEATURE_JNI OFF CACHE BOOL "" FORCE)
# KTX-Software's generic texture entry points retain references to the KTX1
# implementation in the same COFF object as their KTX2 dispatch. MSVC links
# that complete object from the static archive, so omitting KTX1 leaves an
# unresolved ktxTexture1_constructFromStreamAndHeader symbol even when callers
# only create KTX2 textures.
set(KTX_FEATURE_KTX1 ON CACHE BOOL "" FORCE)
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

add_subdirectory("${BLACKFLOWER_KTX_ROOT}" "${CMAKE_CURRENT_BINARY_DIR}/ktx")

include(GNUInstallDirs)
set(BLACKFLOWER_KTX_LIB_DIR "${CMAKE_INSTALL_PREFIX}/${CMAKE_INSTALL_LIBDIR}")
add_custom_target(
    blackflower_ktx_install
    DEPENDS ktx astcenc-none-static
    COMMAND "${CMAKE_COMMAND}" -E make_directory "${BLACKFLOWER_KTX_LIB_DIR}"
    COMMAND "${CMAKE_COMMAND}" -E copy "$<TARGET_FILE:ktx>" "${BLACKFLOWER_KTX_LIB_DIR}/$<TARGET_FILE_NAME:ktx>"
    COMMAND "${CMAKE_COMMAND}" -E copy "$<TARGET_FILE:astcenc-none-static>" "${BLACKFLOWER_KTX_LIB_DIR}/$<TARGET_FILE_NAME:astcenc-none-static>"
    VERBATIM
)
