cmake_minimum_required(VERSION 3.24 FATAL_ERROR)

project(BlackflowerVolumeCooker LANGUAGES CXX)

foreach(input OPENVDB_INSTALL OPENVDB_ROOT BOOST_ROOT OPENVDB_LIBRARY BLOSC_LIBRARY TBB_INSTALL TBB_LIBRARY ZLIB_LIBRARY)
    if(NOT DEFINED BLACKFLOWER_${input})
        message(FATAL_ERROR "Missing BLACKFLOWER_${input}")
    endif()
endforeach()

add_library(openvdb_static STATIC IMPORTED GLOBAL)
set_target_properties(
    openvdb_static
    PROPERTIES
    IMPORTED_LOCATION "${BLACKFLOWER_OPENVDB_LIBRARY}"
    INTERFACE_INCLUDE_DIRECTORIES "${BLACKFLOWER_OPENVDB_INSTALL}/include"
    INTERFACE_COMPILE_DEFINITIONS OPENVDB_STATICLIB
)
add_library(blosc_static STATIC IMPORTED GLOBAL)
set_target_properties(blosc_static PROPERTIES IMPORTED_LOCATION "${BLACKFLOWER_BLOSC_LIBRARY}")
add_library(tbb STATIC IMPORTED GLOBAL)
set_target_properties(
    tbb
    PROPERTIES
    IMPORTED_LOCATION "${BLACKFLOWER_TBB_LIBRARY}"
    INTERFACE_INCLUDE_DIRECTORIES "${BLACKFLOWER_TBB_INSTALL}/include"
)
add_library(zlibstatic STATIC IMPORTED GLOBAL)
set_target_properties(zlibstatic PROPERTIES IMPORTED_LOCATION "${BLACKFLOWER_ZLIB_LIBRARY}")
target_link_libraries(openvdb_static INTERFACE blosc_static tbb zlibstatic)
find_package(Threads REQUIRED)
target_link_libraries(tbb INTERFACE Threads::Threads ${CMAKE_DL_LIBS})

file(GLOB BLACKFLOWER_BOOST_INCLUDE_DIRS
    LIST_DIRECTORIES true
    "${BLACKFLOWER_BOOST_ROOT}/libs/*/include"
    "${BLACKFLOWER_BOOST_ROOT}/libs/numeric/*/include"
)
if(NOT EXISTS "${BLACKFLOWER_BOOST_ROOT}/libs/interprocess/include/boost/interprocess/file_mapping.hpp")
    message(FATAL_ERROR "Boost.Interprocess headers are not initialized")
endif()
target_include_directories(openvdb_static INTERFACE ${BLACKFLOWER_BOOST_INCLUDE_DIRS})

add_executable(blackflower-vdb-cooker wrapper.cpp)
target_compile_features(blackflower-vdb-cooker PRIVATE cxx_std_17)
target_compile_definitions(blackflower-vdb-cooker PRIVATE NANOVDB_USE_OPENVDB)
target_include_directories(
    blackflower-vdb-cooker
    PRIVATE
    "${BLACKFLOWER_OPENVDB_ROOT}/nanovdb"
)
target_link_libraries(blackflower-vdb-cooker PRIVATE openvdb_static)

if(MSVC)
    target_compile_options(blackflower-vdb-cooker PRIVATE /EHsc /bigobj)
endif()

include(GNUInstallDirs)
install(TARGETS blackflower-vdb-cooker RUNTIME DESTINATION "${CMAKE_INSTALL_BINDIR}")
