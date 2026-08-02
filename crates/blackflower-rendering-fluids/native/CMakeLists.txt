cmake_minimum_required(VERSION 3.20 FATAL_ERROR)

project(BlackflowerFlowBindings LANGUAGES CXX)

if(NOT DEFINED BLACKFLOWER_PHYSX_ROOT)
    message(FATAL_ERROR "BLACKFLOWER_PHYSX_ROOT is required")
endif()
if(NOT DEFINED BLACKFLOWER_FLOW_LIBRARY)
    message(FATAL_ERROR "BLACKFLOWER_FLOW_LIBRARY is required")
endif()

set(FLOW_ROOT "${BLACKFLOWER_PHYSX_ROOT}/flow")
add_library(flow_context_opt STATIC IMPORTED GLOBAL)
set_target_properties(
    flow_context_opt
    PROPERTIES
    IMPORTED_LOCATION "${BLACKFLOWER_FLOW_LIBRARY}"
    INTERFACE_INCLUDE_DIRECTORIES "${FLOW_ROOT}/include;${FLOW_ROOT}/include/nvflow;${FLOW_ROOT}/include/nvflowext;${FLOW_ROOT}/shared"
)

add_library(blackflower_flow_wrapper STATIC wrapper.cpp)
target_compile_features(blackflower_flow_wrapper PRIVATE cxx_std_17)
target_link_libraries(blackflower_flow_wrapper PRIVATE flow_context_opt)
if(MSVC)
    target_compile_options(blackflower_flow_wrapper PRIVATE /EHsc /GR-)
else()
    target_compile_options(blackflower_flow_wrapper PRIVATE -fno-exceptions -fno-rtti)
endif()

include(GNUInstallDirs)
install(TARGETS blackflower_flow_wrapper ARCHIVE DESTINATION "${CMAKE_INSTALL_LIBDIR}")
