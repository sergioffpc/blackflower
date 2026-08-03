cmake_minimum_required(VERSION 3.20 FATAL_ERROR)

project(BlackflowerNativeBlast LANGUAGES CXX)

if(NOT DEFINED BLACKFLOWER_PHYSX_ROOT)
    message(FATAL_ERROR "BLACKFLOWER_PHYSX_ROOT is required")
endif()

set(BLAST_ROOT "${BLACKFLOWER_PHYSX_ROOT}/blast")

add_library(
    blackflower_blast
    STATIC
    "${BLAST_ROOT}/source/sdk/common/NvBlastAssert.cpp"
    "${BLAST_ROOT}/source/sdk/common/NvBlastAtomic.cpp"
    "${BLAST_ROOT}/source/sdk/common/NvBlastTime.cpp"
    "${BLAST_ROOT}/source/sdk/common/NvBlastTimers.cpp"
    "${BLAST_ROOT}/source/sdk/globals/NvBlastGlobals.cpp"
    "${BLAST_ROOT}/source/sdk/globals/NvBlastInternalProfiler.cpp"
    "${BLAST_ROOT}/source/sdk/lowlevel/NvBlastActor.cpp"
    "${BLAST_ROOT}/source/sdk/lowlevel/NvBlastActorSerializationBlock.cpp"
    "${BLAST_ROOT}/source/sdk/lowlevel/NvBlastAsset.cpp"
    "${BLAST_ROOT}/source/sdk/lowlevel/NvBlastAssetHelper.cpp"
    "${BLAST_ROOT}/source/sdk/lowlevel/NvBlastFamily.cpp"
    "${BLAST_ROOT}/source/sdk/lowlevel/NvBlastFamilyGraph.cpp"
)
if(CMAKE_SYSTEM_PROCESSOR MATCHES "^(x86_64|AMD64)$")
    set(
        BLAST_STRESS_SOURCES
        "${BLAST_ROOT}/source/sdk/extensions/stress/NvBlastExtStressSolver.cpp"
        "${BLAST_ROOT}/source/shared/stress_solver/stress.cpp"
    )
    target_sources(
        blackflower_blast
        PRIVATE
        ${BLAST_STRESS_SOURCES}
    )
    if(MSVC)
        set_source_files_properties(${BLAST_STRESS_SOURCES} PROPERTIES COMPILE_OPTIONS "/arch:AVX2")
    else()
        set_source_files_properties(${BLAST_STRESS_SOURCES} PROPERTIES COMPILE_OPTIONS "-march=haswell")
    endif()
    target_compile_definitions(blackflower_blast PUBLIC BLACKFLOWER_BLAST_HAS_STRESS=1)
else()
    target_compile_definitions(blackflower_blast PUBLIC BLACKFLOWER_BLAST_HAS_STRESS=0)
endif()
target_compile_features(blackflower_blast PUBLIC cxx_std_17)
target_compile_definitions(
    blackflower_blast
    PUBLIC
    "NV_C_EXPORT=extern \"C\""
    "$<$<CONFIG:Debug>:_DEBUG>"
    "$<$<NOT:$<CONFIG:Debug>>:NDEBUG>"
)
target_include_directories(
    blackflower_blast
    PUBLIC
    "${BLAST_ROOT}/include/lowlevel"
    "${BLAST_ROOT}/include/globals"
    "${BLAST_ROOT}/include/extensions/stress"
    "${BLAST_ROOT}/include/shared/NvFoundation"
    PRIVATE
    "${BLAST_ROOT}/source/sdk/common"
    "${BLAST_ROOT}/source/sdk/globals"
    "${BLAST_ROOT}/source/sdk/lowlevel"
    "${BLAST_ROOT}/source/shared/NsFoundation/include"
    "${BLAST_ROOT}/source/shared/stress_solver"
)
if(APPLE OR MSVC)
    target_include_directories(
        blackflower_blast
        BEFORE PRIVATE
        "${CMAKE_CURRENT_SOURCE_DIR}/compat"
    )
endif()
if(MSVC)
    target_compile_options(
        blackflower_blast
        PRIVATE
        /EHsc
        "/FI${CMAKE_CURRENT_SOURCE_DIR}/compat/blackflower_blast_compat.h"
    )
else()
    target_compile_options(blackflower_blast PRIVATE -Wno-unused-function)
endif()

include(GNUInstallDirs)
install(TARGETS blackflower_blast ARCHIVE DESTINATION "${CMAKE_INSTALL_LIBDIR}")
