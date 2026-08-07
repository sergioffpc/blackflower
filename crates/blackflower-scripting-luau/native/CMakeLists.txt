cmake_minimum_required(VERSION 3.20 FATAL_ERROR)

project(BlackflowerScriptingLuauBindings LANGUAGES CXX)

foreach(input ROOT COMPILER_LIBRARY AST_LIBRARY BYTECODE_LIBRARY CODEGEN_LIBRARY VM_LIBRARY COMMON_LIBRARY)
    if(NOT DEFINED BLACKFLOWER_LUAU_${input})
        message(FATAL_ERROR "Missing BLACKFLOWER_LUAU_${input}")
    endif()
endforeach()
foreach(input USE_LONGJMP IDSIZE VECTOR_SIZE VECTOR_DOUBLE)
    if(NOT DEFINED BLACKFLOWER_LUA_${input})
        message(FATAL_ERROR "Missing BLACKFLOWER_LUA_${input}")
    endif()
endforeach()

foreach(pair
    "Compiler;COMPILER"
    "Ast;AST"
    "Bytecode;BYTECODE"
    "CodeGen;CODEGEN"
    "VM;VM"
    "Common;COMMON"
)
    list(GET pair 0 target_name)
    list(GET pair 1 variable_name)
    add_library(Luau.${target_name} STATIC IMPORTED GLOBAL)
    set_target_properties(
        Luau.${target_name}
        PROPERTIES
        IMPORTED_LOCATION "${BLACKFLOWER_LUAU_${variable_name}_LIBRARY}"
    )
endforeach()
target_compile_definitions(
    Luau.VM
    INTERFACE
    LUA_USE_LONGJMP=${BLACKFLOWER_LUA_USE_LONGJMP}
    LUA_IDSIZE=${BLACKFLOWER_LUA_IDSIZE}
    LUA_VECTOR_SIZE=${BLACKFLOWER_LUA_VECTOR_SIZE}
    LUA_VECTOR_DOUBLE=${BLACKFLOWER_LUA_VECTOR_DOUBLE}
    "LUA_API=extern \"C\""
)
target_compile_definitions(
    Luau.Compiler
    INTERFACE
    "LUACODE_API=extern \"C\""
)
target_compile_definitions(
    Luau.CodeGen
    INTERFACE
    "LUACODEGEN_API=extern \"C\""
)
target_include_directories(
    Luau.CodeGen
    INTERFACE
    "${BLACKFLOWER_LUAU_ROOT}/Common/include"
    "${BLACKFLOWER_LUAU_ROOT}/Ast/include"
    "${BLACKFLOWER_LUAU_ROOT}/Bytecode/include"
    "${BLACKFLOWER_LUAU_ROOT}/CodeGen/include"
    "${BLACKFLOWER_LUAU_ROOT}/Compiler/include"
    "${BLACKFLOWER_LUAU_ROOT}/VM/include"
)

add_library(blackflower_scripting_luau_wrapper STATIC wrapper.cpp)
target_compile_features(blackflower_scripting_luau_wrapper PRIVATE cxx_std_17)
target_compile_definitions(
    blackflower_scripting_luau_wrapper
    PRIVATE
    BF_LUAU_VERSION_MAJOR=0
    BF_LUAU_VERSION_MINOR=731
    BF_LUAU_VERSION_PATCH=0
)
target_include_directories(
    blackflower_scripting_luau_wrapper
    PRIVATE
    "${BLACKFLOWER_LUAU_ROOT}/CodeGen/include"
    "${BLACKFLOWER_LUAU_ROOT}/Compiler/include"
    "${BLACKFLOWER_LUAU_ROOT}/VM/include"
)
target_link_libraries(
    blackflower_scripting_luau_wrapper
    PRIVATE
    Luau.CodeGen
    Luau.Compiler
    Luau.VM
)

include(GNUInstallDirs)
install(
    TARGETS blackflower_scripting_luau_wrapper
    ARCHIVE DESTINATION "${CMAKE_INSTALL_LIBDIR}"
)
