cmake_minimum_required(VERSION 3.24 FATAL_ERROR)

project(BlackflowerNavigationCooker LANGUAGES CXX)

foreach(input RECAST_INSTALL RECAST_LIBRARY DETOUR_LIBRARY)
    if(NOT DEFINED BLACKFLOWER_${input})
        message(FATAL_ERROR "Missing BLACKFLOWER_${input}")
    endif()
endforeach()

add_library(Recast STATIC IMPORTED GLOBAL)
set_target_properties(
    Recast
    PROPERTIES
    IMPORTED_LOCATION "${BLACKFLOWER_RECAST_LIBRARY}"
    INTERFACE_INCLUDE_DIRECTORIES "${BLACKFLOWER_RECAST_INSTALL}/include/recastnavigation"
)
add_library(Detour STATIC IMPORTED GLOBAL)
set_target_properties(
    Detour
    PROPERTIES
    IMPORTED_LOCATION "${BLACKFLOWER_DETOUR_LIBRARY}"
    INTERFACE_INCLUDE_DIRECTORIES "${BLACKFLOWER_RECAST_INSTALL}/include/recastnavigation"
)

add_library(
    blackflower_navigation_cooker
    STATIC
    wrapper.cpp
)
target_compile_features(blackflower_navigation_cooker PRIVATE cxx_std_17)
target_link_libraries(blackflower_navigation_cooker PRIVATE Recast Detour)
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
