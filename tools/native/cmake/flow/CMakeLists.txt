cmake_minimum_required(VERSION 3.20 FATAL_ERROR)

project(BlackflowerNativeFlow LANGUAGES CXX)

if(NOT DEFINED BLACKFLOWER_PHYSX_ROOT)
    message(FATAL_ERROR "BLACKFLOWER_PHYSX_ROOT is required")
endif()

set(FLOW_ROOT "${BLACKFLOWER_PHYSX_ROOT}/flow")

add_library(
    blackflower_flow_context_opt
    STATIC
    "${FLOW_ROOT}/source/nvflowext/opt/ContextOpt.cpp"
    "${FLOW_ROOT}/source/nvflowext/ThreadPool.cpp"
    "${FLOW_ROOT}/shared/NvFlowString.cpp"
)
target_compile_features(blackflower_flow_context_opt PUBLIC cxx_std_17)
target_include_directories(
    blackflower_flow_context_opt
    PUBLIC
    "${FLOW_ROOT}/include"
    "${FLOW_ROOT}/include/nvflow"
    "${FLOW_ROOT}/include/nvflowext"
    "${FLOW_ROOT}/shared"
)

include(GNUInstallDirs)
install(
    TARGETS blackflower_flow_context_opt
    ARCHIVE DESTINATION "${CMAKE_INSTALL_LIBDIR}"
)
