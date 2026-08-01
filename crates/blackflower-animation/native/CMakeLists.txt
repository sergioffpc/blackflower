cmake_minimum_required(VERSION 3.24 FATAL_ERROR)

project(BlackflowerAnimationBindings LANGUAGES CXX)

foreach(input OZZ_ROOT OZZ_ANIMATION_LIBRARY OZZ_BASE_LIBRARY)
    if(NOT DEFINED BLACKFLOWER_${input})
        message(FATAL_ERROR "Missing BLACKFLOWER_${input}")
    endif()
endforeach()

add_library(ozz_animation STATIC IMPORTED GLOBAL)
set_target_properties(
    ozz_animation
    PROPERTIES
    IMPORTED_LOCATION "${BLACKFLOWER_OZZ_ANIMATION_LIBRARY}"
    INTERFACE_INCLUDE_DIRECTORIES "${BLACKFLOWER_OZZ_ROOT}/include"
)
add_library(ozz_base STATIC IMPORTED GLOBAL)
set_target_properties(
    ozz_base
    PROPERTIES
    IMPORTED_LOCATION "${BLACKFLOWER_OZZ_BASE_LIBRARY}"
    INTERFACE_INCLUDE_DIRECTORIES "${BLACKFLOWER_OZZ_ROOT}/include"
)

foreach(version_component MAJOR MINOR PATCH)
    if(NOT DEFINED BLACKFLOWER_OZZ_VERSION_${version_component})
        message(FATAL_ERROR "Missing BLACKFLOWER_OZZ_VERSION_${version_component}")
    endif()
endforeach()

add_library(blackflower_animation_wrapper STATIC wrapper.cpp)
target_compile_features(blackflower_animation_wrapper PRIVATE cxx_std_17)
target_compile_definitions(
    blackflower_animation_wrapper
    PRIVATE
    BF_OZZ_VERSION_MAJOR=${BLACKFLOWER_OZZ_VERSION_MAJOR}
    BF_OZZ_VERSION_MINOR=${BLACKFLOWER_OZZ_VERSION_MINOR}
    BF_OZZ_VERSION_PATCH=${BLACKFLOWER_OZZ_VERSION_PATCH}
)
target_link_libraries(
    blackflower_animation_wrapper
    PRIVATE
    ozz_animation
    ozz_base
)

include(GNUInstallDirs)
install(
    TARGETS blackflower_animation_wrapper
    ARCHIVE DESTINATION "${CMAKE_INSTALL_LIBDIR}"
)
