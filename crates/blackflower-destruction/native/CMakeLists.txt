cmake_minimum_required(VERSION 3.20 FATAL_ERROR)

project(BlackflowerDestructionBindings LANGUAGES CXX)

if(NOT DEFINED BLACKFLOWER_PHYSX_ROOT)
    message(FATAL_ERROR "BLACKFLOWER_PHYSX_ROOT is required")
endif()
if(NOT DEFINED BLACKFLOWER_BLAST_LIBRARY)
    message(FATAL_ERROR "BLACKFLOWER_BLAST_LIBRARY is required")
endif()
if(NOT DEFINED BLACKFLOWER_BLAST_HAS_STRESS)
    message(FATAL_ERROR "BLACKFLOWER_BLAST_HAS_STRESS is required")
endif()

set(BLAST_ROOT "${BLACKFLOWER_PHYSX_ROOT}/blast")
add_library(blast STATIC IMPORTED GLOBAL)
set_target_properties(
    blast
    PROPERTIES
    IMPORTED_LOCATION "${BLACKFLOWER_BLAST_LIBRARY}"
    INTERFACE_INCLUDE_DIRECTORIES "${BLAST_ROOT}/include/lowlevel;${BLAST_ROOT}/include/globals;${BLAST_ROOT}/include/extensions/stress;${BLAST_ROOT}/include/shared/NvFoundation"
)
target_compile_definitions(
    blast
    INTERFACE
    "NV_C_EXPORT=extern \"C\""
    "BLACKFLOWER_BLAST_HAS_STRESS=${BLACKFLOWER_BLAST_HAS_STRESS}"
    "$<$<CONFIG:Debug>:_DEBUG>"
    "$<$<NOT:$<CONFIG:Debug>>:NDEBUG>"
)

add_library(blackflower_destruction_wrapper STATIC wrapper.cpp)
target_compile_features(blackflower_destruction_wrapper PRIVATE cxx_std_17)
target_link_libraries(blackflower_destruction_wrapper PRIVATE blast)
if(MSVC)
    target_compile_options(blackflower_destruction_wrapper PRIVATE /EHsc /GR-)
else()
    target_compile_options(blackflower_destruction_wrapper PRIVATE -fno-exceptions -fno-rtti)
endif()

include(GNUInstallDirs)
install(
    TARGETS blackflower_destruction_wrapper
    ARCHIVE DESTINATION "${CMAKE_INSTALL_LIBDIR}"
)
