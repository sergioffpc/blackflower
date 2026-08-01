cmake_minimum_required(VERSION 3.20 FATAL_ERROR)

project(BlackflowerNativeFlecs LANGUAGES C)

if(NOT DEFINED BLACKFLOWER_FLECS_ROOT)
    message(FATAL_ERROR "BLACKFLOWER_FLECS_ROOT is required")
endif()
if(NOT DEFINED BLACKFLOWER_FLECS_CONFIG_DIR)
    message(FATAL_ERROR "BLACKFLOWER_FLECS_CONFIG_DIR is required")
endif()

add_library(flecs STATIC "${BLACKFLOWER_FLECS_ROOT}/distr/flecs.c")
target_include_directories(
    flecs
    PRIVATE
    "${BLACKFLOWER_FLECS_ROOT}/distr"
    "${BLACKFLOWER_FLECS_CONFIG_DIR}"
)
target_compile_definitions(
    flecs
    PRIVATE
    FLECS_CONFIG_HEADER
    BLACKFLOWER_ECS_ENABLE_STATS
)
if(MSVC)
    target_compile_options(flecs PRIVATE /W0)
else()
    target_compile_options(flecs PRIVATE -w)
endif()

include(GNUInstallDirs)
install(TARGETS flecs ARCHIVE DESTINATION "${CMAKE_INSTALL_LIBDIR}")
install(
    FILES
    "${BLACKFLOWER_FLECS_ROOT}/distr/flecs.h"
    "${BLACKFLOWER_FLECS_CONFIG_DIR}/flecs_config.h"
    DESTINATION "${CMAKE_INSTALL_INCLUDEDIR}/flecs"
)
