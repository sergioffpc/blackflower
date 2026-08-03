cmake_minimum_required(VERSION 3.20 FATAL_ERROR)

project(BlackflowerNativeLuau LANGUAGES CXX)

if(NOT DEFINED BLACKFLOWER_LUAU_ROOT)
    message(FATAL_ERROR "BLACKFLOWER_LUAU_ROOT is required")
endif()

set(LUAU_BUILD_CLI OFF CACHE BOOL "" FORCE)
set(LUAU_BUILD_TESTS OFF CACHE BOOL "" FORCE)
set(LUAU_BUILD_WEB OFF CACHE BOOL "" FORCE)
set(LUAU_BUILD_SHARED OFF CACHE BOOL "" FORCE)
set(LUAU_EXTERN_C ON CACHE BOOL "" FORCE)

add_subdirectory("${BLACKFLOWER_LUAU_ROOT}" "${CMAKE_CURRENT_BINARY_DIR}/luau")

set_target_properties(Luau.Common PROPERTIES OUTPUT_NAME blackflower_luau_common)
set_target_properties(Luau.Ast PROPERTIES OUTPUT_NAME blackflower_luau_ast)
set_target_properties(Luau.Bytecode PROPERTIES OUTPUT_NAME blackflower_luau_bytecode)
set_target_properties(Luau.Compiler PROPERTIES OUTPUT_NAME blackflower_luau_compiler)
set_target_properties(Luau.CodeGen PROPERTIES OUTPUT_NAME blackflower_luau_codegen)
set_target_properties(Luau.VM PROPERTIES OUTPUT_NAME blackflower_luau_vm)

include(GNUInstallDirs)
set(BLACKFLOWER_LUAU_LIB_DIR "${CMAKE_INSTALL_PREFIX}/${CMAKE_INSTALL_LIBDIR}")
add_custom_target(
    blackflower_luau_install
    DEPENDS Luau.Common Luau.Ast Luau.Bytecode Luau.Compiler Luau.CodeGen Luau.VM
    COMMAND "${CMAKE_COMMAND}" -E make_directory "${BLACKFLOWER_LUAU_LIB_DIR}"
    COMMAND "${CMAKE_COMMAND}" -E copy "$<TARGET_FILE:Luau.Common>" "${BLACKFLOWER_LUAU_LIB_DIR}/$<TARGET_FILE_NAME:Luau.Common>"
    COMMAND "${CMAKE_COMMAND}" -E copy "$<TARGET_FILE:Luau.Ast>" "${BLACKFLOWER_LUAU_LIB_DIR}/$<TARGET_FILE_NAME:Luau.Ast>"
    COMMAND "${CMAKE_COMMAND}" -E copy "$<TARGET_FILE:Luau.Bytecode>" "${BLACKFLOWER_LUAU_LIB_DIR}/$<TARGET_FILE_NAME:Luau.Bytecode>"
    COMMAND "${CMAKE_COMMAND}" -E copy "$<TARGET_FILE:Luau.Compiler>" "${BLACKFLOWER_LUAU_LIB_DIR}/$<TARGET_FILE_NAME:Luau.Compiler>"
    COMMAND "${CMAKE_COMMAND}" -E copy "$<TARGET_FILE:Luau.CodeGen>" "${BLACKFLOWER_LUAU_LIB_DIR}/$<TARGET_FILE_NAME:Luau.CodeGen>"
    COMMAND "${CMAKE_COMMAND}" -E copy "$<TARGET_FILE:Luau.VM>" "${BLACKFLOWER_LUAU_LIB_DIR}/$<TARGET_FILE_NAME:Luau.VM>"
    VERBATIM
)
