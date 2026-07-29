cmake_minimum_required(VERSION 3.20 FATAL_ERROR)

project(BlackflowerScriptBindings LANGUAGES CXX)

set(LUAU_BUILD_CLI OFF CACHE BOOL "" FORCE)
set(LUAU_BUILD_TESTS OFF CACHE BOOL "" FORCE)
set(LUAU_BUILD_WEB OFF CACHE BOOL "" FORCE)
set(LUAU_BUILD_SHARED OFF CACHE BOOL "" FORCE)
set(LUAU_EXTERN_C ON CACHE BOOL "" FORCE)
set(LUAU_STATIC_CRT OFF CACHE BOOL "" FORCE)

add_subdirectory(
    "${CMAKE_CURRENT_LIST_DIR}/../vendor/luau"
    "${CMAKE_CURRENT_BINARY_DIR}/luau"
)

set_target_properties(Luau.Common PROPERTIES OUTPUT_NAME blackflower_luau_common)
set_target_properties(Luau.Ast PROPERTIES OUTPUT_NAME blackflower_luau_ast)
set_target_properties(Luau.Bytecode PROPERTIES OUTPUT_NAME blackflower_luau_bytecode)
set_target_properties(Luau.Compiler PROPERTIES OUTPUT_NAME blackflower_luau_compiler)
set_target_properties(Luau.VM PROPERTIES OUTPUT_NAME blackflower_luau_vm)

add_library(blackflower_script_wrapper STATIC wrapper.cpp)
target_compile_features(blackflower_script_wrapper PRIVATE cxx_std_17)
target_compile_definitions(
    blackflower_script_wrapper
    PRIVATE
    BF_LUAU_VERSION_MAJOR=0
    BF_LUAU_VERSION_MINOR=731
    BF_LUAU_VERSION_PATCH=0
)
target_include_directories(
    blackflower_script_wrapper
    PRIVATE
    "${CMAKE_CURRENT_LIST_DIR}/../vendor/luau/Compiler/include"
    "${CMAKE_CURRENT_LIST_DIR}/../vendor/luau/VM/include"
)
target_link_libraries(
    blackflower_script_wrapper
    PRIVATE
    Luau.Compiler
    Luau.VM
)

include(GNUInstallDirs)
install(
    TARGETS
        Luau.Common
        Luau.Ast
        Luau.Bytecode
        Luau.Compiler
        Luau.VM
        blackflower_script_wrapper
    ARCHIVE DESTINATION "${CMAKE_INSTALL_LIBDIR}"
)
