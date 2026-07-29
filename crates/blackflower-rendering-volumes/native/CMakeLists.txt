cmake_minimum_required(VERSION 3.20 FATAL_ERROR)

project(BlackflowerRenderingVolumesNanoVdbBindings LANGUAGES CXX)

foreach(version_component MAJOR MINOR PATCH)
    if(NOT DEFINED BLACKFLOWER_OPENVDB_VERSION_${version_component})
        message(FATAL_ERROR "Missing BLACKFLOWER_OPENVDB_VERSION_${version_component}")
    endif()
endforeach()

add_library(blackflower_rendering_volumes_nanovdb_wrapper STATIC wrapper.cpp)
target_compile_features(blackflower_rendering_volumes_nanovdb_wrapper PRIVATE cxx_std_17)
target_compile_definitions(
    blackflower_rendering_volumes_nanovdb_wrapper
    PRIVATE
    BF_OPENVDB_VERSION_MAJOR=${BLACKFLOWER_OPENVDB_VERSION_MAJOR}
    BF_OPENVDB_VERSION_MINOR=${BLACKFLOWER_OPENVDB_VERSION_MINOR}
    BF_OPENVDB_VERSION_PATCH=${BLACKFLOWER_OPENVDB_VERSION_PATCH}
)
target_include_directories(
    blackflower_rendering_volumes_nanovdb_wrapper
    PRIVATE
    "${CMAKE_CURRENT_LIST_DIR}/../vendor/openvdb/nanovdb"
)

if(MSVC)
    target_compile_options(
        blackflower_rendering_volumes_nanovdb_wrapper
        PRIVATE
        /EHsc
        /GR-
    )
else()
    target_compile_options(
        blackflower_rendering_volumes_nanovdb_wrapper
        PRIVATE
        -fno-rtti
    )
endif()

include(GNUInstallDirs)
install(
    TARGETS blackflower_rendering_volumes_nanovdb_wrapper
    ARCHIVE DESTINATION "${CMAKE_INSTALL_LIBDIR}"
)
