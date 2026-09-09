cmake_minimum_required(VERSION 3.28)

if(NOT DEFINED ENV{VCPKG_ROOT} OR "$ENV{VCPKG_ROOT}" STREQUAL "")
  message(FATAL_ERROR "Set VCPKG_ROOT to the pinned vcpkg checkout. See docs/build.md.")
endif()

project(blackflower LANGUAGES CXX)

if(WIN32)
  # Compiler detection may select llvm-lib; our GNU-style archive rules use ar.
  set(CMAKE_AR "${BLACKFLOWER_LLVM_AR}")
endif()

if(NOT CMAKE_CXX_COMPILER_ID STREQUAL "Clang"
   OR CMAKE_CXX_COMPILER_VERSION VERSION_LESS 21
   OR CMAKE_CXX_COMPILER_VERSION VERSION_GREATER_EQUAL 22)
  message(FATAL_ERROR "Blackflower requires Clang 21. Use a supplied CMake preset.")
endif()

find_program(SCCACHE_EXECUTABLE NAMES sccache REQUIRED)
find_program(CLANG_TIDY_EXECUTABLE NAMES clang-tidy-21 REQUIRED)
find_program(CLANG_FORMAT_EXECUTABLE NAMES clang-format-21 REQUIRED)

option(BLACKFLOWER_USE_SCCACHE "Use sccache for project compilation" ON)
if(BLACKFLOWER_USE_SCCACHE)
  set(CMAKE_CXX_COMPILER_LAUNCHER "${SCCACHE_EXECUTABLE}")
else()
  # Build tracers such as CodeQL must observe the compiler directly.
  set(CMAKE_CXX_COMPILER_LAUNCHER "")
endif()
set(CMAKE_EXPORT_COMPILE_COMMANDS ON)
set(CMAKE_CXX_SCAN_FOR_MODULES OFF)
include(cmake/Sanitizers.cmake)

find_package(GTest CONFIG REQUIRED)
find_package(benchmark CONFIG REQUIRED)
find_package(unofficial-sodium CONFIG REQUIRED)
find_package(Boost REQUIRED COMPONENTS filesystem iostreams)

add_library(blackflower_content STATIC src/modules/content/content.cc)
add_library(blackflower_content_mapping STATIC src/modules/content/mapped_file.cc)
target_link_libraries(blackflower_content_mapping PRIVATE Boost::filesystem Boost::iostreams)
target_link_libraries(blackflower_content PRIVATE blackflower_content_mapping)
target_include_directories(blackflower_content PUBLIC src/modules/content)
target_link_libraries(blackflower_content PRIVATE unofficial-sodium::sodium)
add_executable(blackflower_content_harness tests/content_harness.cc)
target_link_libraries(blackflower_content_harness PRIVATE blackflower_content)

add_executable(blackflower src/main.cc)
add_executable(blackflower_tests tests/build_test.cc tests/content_test.cc)
target_link_libraries(blackflower_tests PRIVATE GTest::gtest_main blackflower_content)
add_executable(blackflower_benchmarks benchmarks/framework_benchmark.cc)
target_link_libraries(blackflower_benchmarks PRIVATE benchmark::benchmark_main)

set(BLACKFLOWER_TARGETS blackflower blackflower_tests blackflower_benchmarks
    blackflower_content blackflower_content_harness blackflower_content_mapping)
set(BLACKFLOWER_SOURCES)
set(BLACKFLOWER_HEADERS src/modules/content/content.h src/modules/content/mapped_file.h)
foreach(target IN LISTS BLACKFLOWER_TARGETS)
  target_link_libraries(${target} PRIVATE blackflower_sanitizers)
  set_target_properties(${target} PROPERTIES
    CXX_STANDARD 23
    CXX_STANDARD_REQUIRED YES
    CXX_EXTENSIONS NO
  )
  target_compile_options(${target} PRIVATE
    -Wall -Wextra -Wpedantic -Werror
  )
  if(target STREQUAL "blackflower_content_mapping")
    target_compile_options(${target} PRIVATE -fexceptions)
    if(WIN32)
      target_compile_definitions(${target} PRIVATE _HAS_EXCEPTIONS=1)
    endif()
  else()
    target_compile_options(${target} PRIVATE -fno-exceptions)
    if(WIN32)
      target_compile_definitions(${target} PRIVATE _HAS_EXCEPTIONS=0)
    endif()
  endif()
  get_target_property(target_sources ${target} SOURCES)
  list(APPEND BLACKFLOWER_SOURCES ${target_sources})
endforeach()

if(NOT CMAKE_CROSSCOMPILING)
  enable_testing()
  find_program(UV_EXECUTABLE NAMES uv REQUIRED)
  add_test(NAME content.pipeline
    COMMAND "${CMAKE_COMMAND}" -E env
      "BLACKFLOWER_CONTENT_HARNESS=$<TARGET_FILE:blackflower_content_harness>"
      "${UV_EXECUTABLE}" run --locked --no-sync
      --project "${PROJECT_SOURCE_DIR}/tools/cooker"
      python "${PROJECT_SOURCE_DIR}/tests/integration/content_pipeline_test.py")
  add_test(NAME content.typecheck
    COMMAND "${UV_EXECUTABLE}" run --locked --no-sync
      --project "${PROJECT_SOURCE_DIR}/tools/cooker" mypy
      --config-file "${PROJECT_SOURCE_DIR}/tools/cooker/pyproject.toml"
      "${PROJECT_SOURCE_DIR}/tools/cooker/src"
      "${PROJECT_SOURCE_DIR}/tests/integration/content_pipeline_test.py")
  set_tests_properties(content.pipeline PROPERTIES TIMEOUT 120)
  add_test(NAME content.format
    COMMAND "${UV_EXECUTABLE}" run --locked --no-sync
      --project "${PROJECT_SOURCE_DIR}/tools/cooker" pyink --check
      --config "${PROJECT_SOURCE_DIR}/tools/cooker/pyproject.toml"
      "${PROJECT_SOURCE_DIR}/tools/cooker/src"
      "${PROJECT_SOURCE_DIR}/tests/integration")
  add_test(NAME content.lint
    COMMAND "${UV_EXECUTABLE}" run --locked --no-sync
      --project "${PROJECT_SOURCE_DIR}/tools/cooker" pylint
      "--rcfile=${PROJECT_SOURCE_DIR}/.pylintrc"
      "${PROJECT_SOURCE_DIR}/tools/cooker/src/content"
      "${PROJECT_SOURCE_DIR}/tests/integration/content_pipeline_test.py")
  set_tests_properties(content.format content.lint PROPERTIES
    WORKING_DIRECTORY "${PROJECT_SOURCE_DIR}" TIMEOUT 120)
  include(GoogleTest)
  foreach(fixture IN ITEMS reference.bfserver reference.pub)
    configure_file("tests/fixtures/packs/${fixture}"
      "test-fixtures/${fixture}" COPYONLY)
  endforeach()
  gtest_discover_tests(blackflower_tests
    WORKING_DIRECTORY "${PROJECT_BINARY_DIR}/test-fixtures")
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
  COMMAND "${CLANG_FORMAT_EXECUTABLE}" --dry-run --Werror ${BLACKFLOWER_SOURCES} ${BLACKFLOWER_HEADERS}
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
