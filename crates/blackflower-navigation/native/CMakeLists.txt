cmake_minimum_required(VERSION 3.20 FATAL_ERROR)

project(BlackflowerNavigationBindings LANGUAGES CXX)

set(DETOUR_ROOT "${CMAKE_CURRENT_LIST_DIR}/../vendor/recastnavigation/Detour")

foreach(version_component MAJOR MINOR PATCH)
    if(NOT DEFINED BLACKFLOWER_RECAST_VERSION_${version_component})
        message(FATAL_ERROR "Missing BLACKFLOWER_RECAST_VERSION_${version_component}")
    endif()
endforeach()

add_library(
    Detour
    STATIC
    "${DETOUR_ROOT}/Source/DetourAlloc.cpp"
    "${DETOUR_ROOT}/Source/DetourAssert.cpp"
    "${DETOUR_ROOT}/Source/DetourCommon.cpp"
    "${DETOUR_ROOT}/Source/DetourNavMesh.cpp"
    "${DETOUR_ROOT}/Source/DetourNavMeshQuery.cpp"
    "${DETOUR_ROOT}/Source/DetourNode.cpp"
)
target_include_directories(Detour PUBLIC "${DETOUR_ROOT}/Include")
target_compile_features(Detour PRIVATE cxx_std_17)

add_library(blackflower_navigation_wrapper STATIC wrapper.cpp)
target_compile_features(blackflower_navigation_wrapper PRIVATE cxx_std_17)
target_compile_definitions(
    blackflower_navigation_wrapper
    PRIVATE
    BF_RECAST_VERSION_MAJOR=${BLACKFLOWER_RECAST_VERSION_MAJOR}
    BF_RECAST_VERSION_MINOR=${BLACKFLOWER_RECAST_VERSION_MINOR}
    BF_RECAST_VERSION_PATCH=${BLACKFLOWER_RECAST_VERSION_PATCH}
)
target_link_libraries(blackflower_navigation_wrapper PRIVATE Detour)

foreach(target Detour blackflower_navigation_wrapper)
    if(MSVC)
        target_compile_options(${target} PRIVATE /GR-)
    else()
        target_compile_options(${target} PRIVATE -fno-exceptions -fno-rtti)
    endif()
endforeach()

include(GNUInstallDirs)
install(
    TARGETS Detour blackflower_navigation_wrapper
    ARCHIVE DESTINATION "${CMAKE_INSTALL_LIBDIR}"
)
