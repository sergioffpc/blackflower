cmake_minimum_required(VERSION 3.20 FATAL_ERROR)

project(BlackflowerNavigationBindings LANGUAGES CXX)

if(NOT DEFINED BLACKFLOWER_RECAST_INSTALL)
    message(FATAL_ERROR "BLACKFLOWER_RECAST_INSTALL is required")
endif()
if(NOT DEFINED BLACKFLOWER_DETOUR_LIBRARY)
    message(FATAL_ERROR "BLACKFLOWER_DETOUR_LIBRARY is required")
endif()

foreach(version_component MAJOR MINOR PATCH)
    if(NOT DEFINED BLACKFLOWER_RECAST_VERSION_${version_component})
        message(FATAL_ERROR "Missing BLACKFLOWER_RECAST_VERSION_${version_component}")
    endif()
endforeach()

add_library(Detour STATIC IMPORTED GLOBAL)
set_target_properties(
    Detour
    PROPERTIES
    IMPORTED_LOCATION "${BLACKFLOWER_DETOUR_LIBRARY}"
    INTERFACE_INCLUDE_DIRECTORIES "${BLACKFLOWER_RECAST_INSTALL}/include/recastnavigation"
)

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

foreach(target blackflower_navigation_wrapper)
    if(MSVC)
        target_compile_options(${target} PRIVATE /GR-)
    else()
        target_compile_options(${target} PRIVATE -fno-exceptions -fno-rtti)
    endif()
endforeach()

include(GNUInstallDirs)
install(
    TARGETS blackflower_navigation_wrapper
    ARCHIVE DESTINATION "${CMAKE_INSTALL_LIBDIR}"
)
