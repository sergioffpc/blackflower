cmake_minimum_required(VERSION 3.20 FATAL_ERROR)

project(BlackflowerPhysicsBindings LANGUAGES CXX)

add_subdirectory(
    "${CMAKE_CURRENT_LIST_DIR}/../vendor/JoltPhysics/Build"
    "${CMAKE_CURRENT_BINARY_DIR}/jolt"
)

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
install(
    TARGETS Jolt blackflower_physics_wrapper
    ARCHIVE DESTINATION "${CMAKE_INSTALL_LIBDIR}"
)
