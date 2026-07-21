cmake_minimum_required(VERSION 3.20)
project(roadsim_sumo_bridge LANGUAGES CXX)

set(ROADSIM_SUMO_HOME "" CACHE PATH "Exact SUMO source/build root")
set(ROADSIM_SUMO_VERSION "1.27.1" CACHE STRING "Exact SUMO runtime version")
set(ROADSIM_SUMO_LIBRARY_DIR "" CACHE PATH "Directory containing libsumocpp")
set(ROADSIM_SUMO_SOURCE_REVISION
    "7717f2379d9e314a0c81c5cec748444de06a2a91"
    CACHE STRING "Exact SUMO source commit")

if(NOT ROADSIM_SUMO_HOME)
    message(FATAL_ERROR "ROADSIM_SUMO_HOME is required")
endif()
if(NOT ROADSIM_SUMO_LIBRARY_DIR)
    set(ROADSIM_SUMO_LIBRARY_DIR "${ROADSIM_SUMO_HOME}/bin")
endif()

find_path(SUMO_INCLUDE_DIR
          NAMES libsumo/Simulation.h
          HINTS "${ROADSIM_SUMO_HOME}/src"
          REQUIRED)
find_library(SUMO_CPP_LIBRARY
             NAMES sumocpp libsumocpp
             HINTS "${ROADSIM_SUMO_LIBRARY_DIR}" "${ROADSIM_SUMO_HOME}/lib"
             REQUIRED)

add_library(roadsim_sumo_bridge SHARED roadsim_sumo_bridge.cpp)
target_compile_features(roadsim_sumo_bridge PRIVATE cxx_std_17)
target_include_directories(roadsim_sumo_bridge PRIVATE "${SUMO_INCLUDE_DIR}")
target_link_libraries(roadsim_sumo_bridge PRIVATE "${SUMO_CPP_LIBRARY}")
target_compile_definitions(
    roadsim_sumo_bridge
    PRIVATE ROADSIM_SUMO_VERSION="${ROADSIM_SUMO_VERSION}"
            ROADSIM_SUMO_SOURCE_REVISION="${ROADSIM_SUMO_SOURCE_REVISION}")
set_target_properties(roadsim_sumo_bridge PROPERTIES CXX_VISIBILITY_PRESET hidden)
set_target_properties(roadsim_sumo_bridge PROPERTIES BUILD_RPATH "${ROADSIM_SUMO_LIBRARY_DIR}")
