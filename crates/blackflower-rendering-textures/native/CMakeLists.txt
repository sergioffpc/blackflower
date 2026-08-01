cmake_minimum_required(VERSION 3.22 FATAL_ERROR)

project(BlackflowerTextureBindings LANGUAGES C CXX)

foreach(input KTX_VERSION KTX_ROOT KTX_LIBRARY)
    if(NOT DEFINED BLACKFLOWER_${input})
        message(FATAL_ERROR "BLACKFLOWER_${input} is required")
    endif()
endforeach()

add_library(ktx STATIC IMPORTED GLOBAL)
set_target_properties(
    ktx
    PROPERTIES
    IMPORTED_LOCATION "${BLACKFLOWER_KTX_LIBRARY}"
    INTERFACE_INCLUDE_DIRECTORIES "${BLACKFLOWER_KTX_ROOT}/include;${BLACKFLOWER_KTX_ROOT}/lib"
)

add_library(blackflower_texture_wrapper STATIC wrapper.cpp)
target_compile_features(blackflower_texture_wrapper PRIVATE cxx_std_17)
target_compile_definitions(
    blackflower_texture_wrapper
    PRIVATE
        KHRONOS_STATIC
        KTX_FEATURE_WRITE
        BF_KTX_VERSION="${BLACKFLOWER_KTX_VERSION}"
)
target_include_directories(
    blackflower_texture_wrapper
    PRIVATE
        "${BLACKFLOWER_KTX_ROOT}/include"
        "${BLACKFLOWER_KTX_ROOT}/lib"
)
target_link_libraries(blackflower_texture_wrapper PRIVATE ktx)

include(GNUInstallDirs)
install(
    TARGETS blackflower_texture_wrapper
    ARCHIVE DESTINATION "${CMAKE_INSTALL_LIBDIR}"
)
