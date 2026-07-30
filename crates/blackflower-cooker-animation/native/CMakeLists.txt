cmake_minimum_required(VERSION 3.24 FATAL_ERROR)

project(BlackflowerAnimationCooker LANGUAGES CXX)

set(BUILD_SHARED_LIBS OFF CACHE BOOL "" FORCE)
set(ozz_build_data OFF CACHE BOOL "" FORCE)
set(ozz_build_fbx OFF CACHE BOOL "" FORCE)
set(ozz_build_gltf ON CACHE BOOL "" FORCE)
set(ozz_build_howtos OFF CACHE BOOL "" FORCE)
set(ozz_build_msvc_rt_dll ON CACHE BOOL "" FORCE)
set(ozz_build_postfix OFF CACHE BOOL "" FORCE)
set(ozz_build_samples OFF CACHE BOOL "" FORCE)
set(ozz_build_tests OFF CACHE BOOL "" FORCE)
set(ozz_build_tools ON CACHE BOOL "" FORCE)

add_compile_options($<$<CXX_COMPILER_ID:MSVC>:/EHsc>)

add_subdirectory(
    "${CMAKE_CURRENT_LIST_DIR}/../../blackflower-animation/vendor/ozz-animation"
    "${CMAKE_CURRENT_BINARY_DIR}/ozz"
)
