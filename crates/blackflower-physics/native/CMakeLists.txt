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
    "$<$<NOT:$<CONFIG:Debug>>:NDEBUG>"
)
if(CMAKE_SYSTEM_NAME STREQUAL "Linux" AND CMAKE_SYSTEM_PROCESSOR MATCHES "^(x86_64|AMD64)$")
    target_compile_definitions(Jolt INTERFACE JPH_USE_AVX2)
endif()

add_library(blackflower_physics_wrapper STATIC wrapper.cpp)
target_compile_features(blackflower_physics_wrapper PRIVATE cxx_std_17)
target_link_libraries(blackflower_physics_wrapper PRIVATE Jolt)

if(MSVC)
    target_compile_options(blackflower_physics_wrapper PRIVATE /GR-)
else()
    target_compile_options(
        blackflower_physics_wrapper
        PRIVATE
        -fno-exceptions
        -fno-rtti
    )
endif()

include(GNUInstallDirs)
install(TARGETS blackflower_physics_wrapper ARCHIVE DESTINATION "${CMAKE_INSTALL_LIBDIR}")
