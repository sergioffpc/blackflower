cmake_minimum_required(VERSION 3.24 FATAL_ERROR)

project(BlackflowerNavigationCooker LANGUAGES CXX)

if(NOT DEFINED BLACKFLOWER_RECAST_ROOT)
    message(FATAL_ERROR "Missing BLACKFLOWER_RECAST_ROOT")
endif()

set(RECAST_ROOT "${BLACKFLOWER_RECAST_ROOT}/Recast")
set(DETOUR_ROOT "${BLACKFLOWER_RECAST_ROOT}/Detour")

add_library(
    blackflower_navigation_cooker
    STATIC
    cooker.cpp
    "${RECAST_ROOT}/Source/Recast.cpp"
    "${RECAST_ROOT}/Source/RecastAlloc.cpp"
    "${RECAST_ROOT}/Source/RecastArea.cpp"
    "${RECAST_ROOT}/Source/RecastAssert.cpp"
    "${RECAST_ROOT}/Source/RecastContour.cpp"
    "${RECAST_ROOT}/Source/RecastFilter.cpp"
    "${RECAST_ROOT}/Source/RecastLayers.cpp"
    "${RECAST_ROOT}/Source/RecastMesh.cpp"
    "${RECAST_ROOT}/Source/RecastMeshDetail.cpp"
    "${RECAST_ROOT}/Source/RecastRasterization.cpp"
    "${RECAST_ROOT}/Source/RecastRegion.cpp"
    "${DETOUR_ROOT}/Source/DetourAlloc.cpp"
    "${DETOUR_ROOT}/Source/DetourAssert.cpp"
    "${DETOUR_ROOT}/Source/DetourCommon.cpp"
    "${DETOUR_ROOT}/Source/DetourNavMeshBuilder.cpp"
)
target_include_directories(
    blackflower_navigation_cooker
    PRIVATE
    "${RECAST_ROOT}/Include"
    "${DETOUR_ROOT}/Include"
)
target_compile_features(blackflower_navigation_cooker PRIVATE cxx_std_17)
if(MSVC)
    target_compile_options(blackflower_navigation_cooker PRIVATE /GR-)
else()
    target_compile_options(
        blackflower_navigation_cooker
        PRIVATE
        -fno-exceptions
        -fno-rtti
    )
endif()

include(GNUInstallDirs)
install(
    TARGETS blackflower_navigation_cooker
    ARCHIVE DESTINATION "${CMAKE_INSTALL_LIBDIR}"
)
