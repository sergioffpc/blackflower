add_library(blackflower_sanitizers INTERFACE)

# Release measurements must use production code without instrumentation.
if(CMAKE_BUILD_TYPE STREQUAL "Release")
  return()
endif()

if(WIN32)
  set(default_sanitizer address)
else()
  set(default_sanitizer address-undefined)
endif()
set(BLACKFLOWER_SANITIZER "${default_sanitizer}" CACHE STRING
    "Non-Release sanitizer: address-undefined, address, or thread")
set_property(CACHE BLACKFLOWER_SANITIZER PROPERTY STRINGS
             address-undefined address thread)

if(BLACKFLOWER_SANITIZER STREQUAL "address-undefined" AND NOT WIN32)
  set(sanitizer_flags -fsanitize=address,undefined -fno-sanitize-recover=all)
elseif(BLACKFLOWER_SANITIZER STREQUAL "thread" AND NOT WIN32)
  set(sanitizer_flags -fsanitize=thread)
elseif(BLACKFLOWER_SANITIZER STREQUAL "address")
  set(sanitizer_flags -fsanitize=address)
else()
  message(FATAL_ERROR "Unsupported sanitizer for this target: ${BLACKFLOWER_SANITIZER}")
endif()

target_compile_options(blackflower_sanitizers INTERFACE
  ${sanitizer_flags} -fno-omit-frame-pointer
)
if(NOT WIN32)
  target_link_options(blackflower_sanitizers INTERFACE ${sanitizer_flags})
endif()

if(WIN32)
  # Match the uninstrumented vcpkg libraries' MSVC STL annotation ABI.
  target_compile_definitions(blackflower_sanitizers INTERFACE _DISABLE_STL_ANNOTATION)
  set(asan_dir "$ENV{LLVM_WINDOWS_ASAN_DIR}")
  foreach(runtime clang_rt.asan_dynamic-x86_64.lib
                  clang_rt.asan_dynamic_runtime_thunk-x86_64.lib
                  clang_rt.asan_dynamic-x86_64.dll)
    if(NOT EXISTS "${asan_dir}/${runtime}")
      message(FATAL_ERROR
        "Set LLVM_WINDOWS_ASAN_DIR to the LLVM 21 Windows ASan runtimes. See docs/build.md.")
    endif()
  endforeach()
  target_link_options(blackflower_sanitizers INTERFACE
    "SHELL:-Xlinker /wholearchive:\"${asan_dir}/clang_rt.asan_dynamic_runtime_thunk-x86_64.lib\""
  )
  target_link_libraries(blackflower_sanitizers INTERFACE
    "${asan_dir}/clang_rt.asan_dynamic-x86_64.lib"
  )
  configure_file("${asan_dir}/clang_rt.asan_dynamic-x86_64.dll"
                 "${PROJECT_BINARY_DIR}/clang_rt.asan_dynamic-x86_64.dll" COPYONLY)
endif()
