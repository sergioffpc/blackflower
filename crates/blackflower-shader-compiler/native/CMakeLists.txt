cmake_minimum_required(VERSION 3.22 FATAL_ERROR)

project(BlackflowerShaderCompilerBindings LANGUAGES CXX)

if(NOT DEFINED BLACKFLOWER_SLANG_ROOT)
    message(FATAL_ERROR "BLACKFLOWER_SLANG_ROOT is required")
endif()
if(NOT DEFINED BLACKFLOWER_SLANG_LIBRARY)
    message(FATAL_ERROR "BLACKFLOWER_SLANG_LIBRARY is required")
endif()

add_library(slang STATIC IMPORTED GLOBAL)
set_target_properties(
    slang
    PROPERTIES
    IMPORTED_LOCATION "${BLACKFLOWER_SLANG_LIBRARY}"
    INTERFACE_INCLUDE_DIRECTORIES "${BLACKFLOWER_SLANG_ROOT}/include"
)

add_library(blackflower_shader_compiler_wrapper STATIC wrapper.cpp)
target_compile_features(blackflower_shader_compiler_wrapper PRIVATE cxx_std_17)
target_compile_definitions(
    blackflower_shader_compiler_wrapper
    PRIVATE BF_SLANG_VERSION="2026.14.1"
)
target_link_libraries(blackflower_shader_compiler_wrapper PRIVATE slang)

include(GNUInstallDirs)
install(
    TARGETS blackflower_shader_compiler_wrapper
    ARCHIVE DESTINATION "${CMAKE_INSTALL_LIBDIR}"
)
