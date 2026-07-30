cmake_minimum_required(VERSION 3.20 FATAL_ERROR)

project(BlackflowerScriptingBindings LANGUAGES CXX)

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
set_target_properties(Luau.CodeGen PROPERTIES OUTPUT_NAME blackflower_luau_codegen)
set_target_properties(Luau.VM PROPERTIES OUTPUT_NAME blackflower_luau_vm)

add_library(blackflower_scripting_wrapper STATIC wrapper.cpp)
target_compile_features(blackflower_scripting_wrapper PRIVATE cxx_std_17)
target_compile_definitions(
    blackflower_scripting_wrapper
    PRIVATE
    BF_LUAU_VERSION_MAJOR=0
    BF_LUAU_VERSION_MINOR=731
    BF_LUAU_VERSION_PATCH=0
)
target_include_directories(
    blackflower_scripting_wrapper
    PRIVATE
    "${CMAKE_CURRENT_LIST_DIR}/../vendor/luau/CodeGen/include"
    "${CMAKE_CURRENT_LIST_DIR}/../vendor/luau/Compiler/include"
    "${CMAKE_CURRENT_LIST_DIR}/../vendor/luau/VM/include"
)
target_link_libraries(
    blackflower_scripting_wrapper
    PRIVATE
    Luau.CodeGen
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
        Luau.CodeGen
        Luau.VM
        blackflower_scripting_wrapper
    ARCHIVE DESTINATION "${CMAKE_INSTALL_LIBDIR}"
)
