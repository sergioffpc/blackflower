cmake_minimum_required(VERSION 3.20 FATAL_ERROR)

project(BlackflowerNativeJolt LANGUAGES CXX)

if(NOT DEFINED BLACKFLOWER_JOLT_ROOT)
    message(FATAL_ERROR "BLACKFLOWER_JOLT_ROOT is required")
endif()

add_subdirectory("${BLACKFLOWER_JOLT_ROOT}/Build" "${CMAKE_CURRENT_BINARY_DIR}/jolt")

# Compile the probe as part of Jolt itself so it captures the archive's exact public ABI defines.
target_sources(Jolt PRIVATE configuration.cpp)

include(GNUInstallDirs)
install(TARGETS Jolt ARCHIVE DESTINATION "${CMAKE_INSTALL_LIBDIR}")
