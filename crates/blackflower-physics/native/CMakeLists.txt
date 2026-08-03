cmake_minimum_required(VERSION 3.20 FATAL_ERROR)

project(BlackflowerPhysicsBindings LANGUAGES CXX)

if(NOT DEFINED BLACKFLOWER_JOLT_ROOT)
    message(FATAL_ERROR "BLACKFLOWER_JOLT_ROOT is required")
endif()
if(NOT DEFINED BLACKFLOWER_JOLT_LIBRARY)
    message(FATAL_ERROR "BLACKFLOWER_JOLT_LIBRARY is required")
endif()

add_library(Jolt STATIC IMPORTED GLOBAL)
set_target_properties(
    Jolt
    PROPERTIES
    IMPORTED_LOCATION "${BLACKFLOWER_JOLT_LIBRARY}"
    INTERFACE_INCLUDE_DIRECTORIES "${BLACKFLOWER_JOLT_ROOT}"
)
target_compile_definitions(
    Jolt
    INTERFACE
    JPH_CROSS_PLATFORM_DETERMINISTIC
    BF_JOLT_STRICT_FLOAT=1
    "$<$<NOT:$<CONFIG:Debug>>:NDEBUG>"
)
if(CMAKE_SYSTEM_NAME STREQUAL "Linux" AND CMAKE_SYSTEM_PROCESSOR MATCHES "^(x86_64|AMD64)$")
    target_compile_definitions(Jolt INTERFACE JPH_USE_AVX2)
    target_compile_options(
        Jolt
        INTERFACE
        -mavx2
        -mbmi
        -mpopcnt
        -mlzcnt
        -mf16c
        -mfpmath=sse
    )
endif()

add_library(blackflower_physics_wrapper STATIC wrapper.cpp)
target_compile_features(blackflower_physics_wrapper PRIVATE cxx_std_17)
target_link_libraries(blackflower_physics_wrapper PRIVATE Jolt)

if(MSVC)
    target_compile_options(
        blackflower_physics_wrapper
        PRIVATE
        /EHsc
        /GR-
        /fp:strict
    )
else()
    target_compile_options(
        blackflower_physics_wrapper
        PRIVATE
        -fexceptions
        -ffp-contract=off
        -fno-fast-math
        -fno-rtti
    )
endif()

include(GNUInstallDirs)
install(TARGETS blackflower_physics_wrapper ARCHIVE DESTINATION "${CMAKE_INSTALL_LIBDIR}")
