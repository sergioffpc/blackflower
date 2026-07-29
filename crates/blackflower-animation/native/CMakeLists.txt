cmake_minimum_required(VERSION 3.24 FATAL_ERROR)

project(BlackflowerAnimationBindings LANGUAGES CXX)

set(BUILD_SHARED_LIBS OFF CACHE BOOL "" FORCE)
set(ozz_build_data OFF CACHE BOOL "" FORCE)
set(ozz_build_fbx OFF CACHE BOOL "" FORCE)
set(ozz_build_gltf OFF CACHE BOOL "" FORCE)
set(ozz_build_howtos OFF CACHE BOOL "" FORCE)
set(ozz_build_msvc_rt_dll ON CACHE BOOL "" FORCE)
set(ozz_build_postfix OFF CACHE BOOL "" FORCE)
set(ozz_build_samples OFF CACHE BOOL "" FORCE)
set(ozz_build_tests OFF CACHE BOOL "" FORCE)
set(ozz_build_tools OFF CACHE BOOL "" FORCE)

add_compile_options($<$<CXX_COMPILER_ID:MSVC>:/EHsc>)

add_subdirectory(
    "${CMAKE_CURRENT_LIST_DIR}/../vendor/ozz-animation"
    "${CMAKE_CURRENT_BINARY_DIR}/ozz"
    EXCLUDE_FROM_ALL
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
    TARGETS blackflower_animation_wrapper ozz_animation ozz_base
    ARCHIVE DESTINATION "${CMAKE_INSTALL_LIBDIR}"
)
