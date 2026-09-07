cmake_minimum_required(VERSION 3.28)

if(NOT DEFINED ENV{VCPKG_ROOT} OR "$ENV{VCPKG_ROOT}" STREQUAL "")
  message(FATAL_ERROR "Set VCPKG_ROOT to the pinned vcpkg checkout. See docs/build.md.")
endif()

project(blackflower LANGUAGES CXX)

if(NOT CMAKE_CXX_COMPILER_ID STREQUAL "Clang"
   OR CMAKE_CXX_COMPILER_VERSION VERSION_LESS 21
   OR CMAKE_CXX_COMPILER_VERSION VERSION_GREATER_EQUAL 22)
  message(FATAL_ERROR "Blackflower requires Clang 21. Use a supplied CMake preset.")
endif()

find_program(SCCACHE_EXECUTABLE NAMES sccache REQUIRED)
find_program(CLANG_TIDY_EXECUTABLE NAMES clang-tidy-21 REQUIRED)
find_program(CLANG_FORMAT_EXECUTABLE NAMES clang-format-21 REQUIRED)

set(CMAKE_CXX_COMPILER_LAUNCHER "${SCCACHE_EXECUTABLE}")
set(CMAKE_EXPORT_COMPILE_COMMANDS ON)
set(CMAKE_CXX_SCAN_FOR_MODULES OFF)
include(cmake/Sanitizers.cmake)

find_package(GTest CONFIG REQUIRED)
find_package(benchmark CONFIG REQUIRED)

add_executable(blackflower src/main.cc)
add_executable(blackflower_tests tests/build_test.cc)
target_link_libraries(blackflower_tests PRIVATE GTest::gtest_main)
add_executable(blackflower_benchmarks benchmarks/framework_benchmark.cc)
target_link_libraries(blackflower_benchmarks PRIVATE benchmark::benchmark_main)

set(BLACKFLOWER_TARGETS blackflower blackflower_tests blackflower_benchmarks)
set(BLACKFLOWER_SOURCES)
foreach(target IN LISTS BLACKFLOWER_TARGETS)
  target_link_libraries(${target} PRIVATE blackflower_sanitizers)
  set_target_properties(${target} PROPERTIES
    CXX_STANDARD 23
    CXX_STANDARD_REQUIRED YES
    CXX_EXTENSIONS NO
  )
  target_compile_options(${target} PRIVATE
    -Wall -Wextra -Wpedantic -Werror -fno-exceptions
  )
  if(WIN32)
    target_compile_definitions(${target} PRIVATE _HAS_EXCEPTIONS=0)
  endif()
  get_target_property(target_sources ${target} SOURCES)
  list(APPEND BLACKFLOWER_SOURCES ${target_sources})
endforeach()

if(NOT CMAKE_CROSSCOMPILING)
  enable_testing()
  include(GoogleTest)
  gtest_discover_tests(blackflower_tests)
  add_test(NAME blackflower.smoke COMMAND blackflower)
  add_test(NAME blackflower.benchmark_smoke
    COMMAND blackflower_benchmarks --benchmark_filter=FrameworkSmoke
            --benchmark_min_time=0.001s
  )
  set_tests_properties(blackflower.smoke blackflower.benchmark_smoke PROPERTIES
    TIMEOUT 30
  )
endif()

add_custom_target(analyze
  COMMAND "${CLANG_TIDY_EXECUTABLE}"
          "--config-file=${PROJECT_SOURCE_DIR}/.clang-tidy" --verify-config
  COMMAND "${CLANG_TIDY_EXECUTABLE}"
          "--config-file=${PROJECT_SOURCE_DIR}/.clang-tidy"
          -p "${PROJECT_BINARY_DIR}" ${BLACKFLOWER_SOURCES}
  COMMAND "${CLANG_FORMAT_EXECUTABLE}" --dry-run --Werror ${BLACKFLOWER_SOURCES}
  DEPENDS ${BLACKFLOWER_TARGETS}
  WORKING_DIRECTORY "${PROJECT_SOURCE_DIR}"
  COMMENT "Checking C++ analysis and formatting"
  VERBATIM
)

if(NOT CMAKE_CROSSCOMPILING)
  add_custom_target(check
    COMMAND "${CMAKE_CTEST_COMMAND}" --test-dir "${PROJECT_BINARY_DIR}"
            --output-on-failure --no-tests=error
    DEPENDS analyze
    COMMENT "Running tests and benchmark startup"
    VERBATIM
  )
else()
  message(STATUS "Cross-build: use analyze, then run the executables on Windows.")
endif()
