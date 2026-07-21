#pragma once

#include <cstddef>
#include <cstdint>

#if defined(_WIN32)
#define ROADSIM_SUMO_EXPORT __declspec(dllexport)
#else
#define ROADSIM_SUMO_EXPORT __attribute__((visibility("default")))
#endif

extern "C" {

ROADSIM_SUMO_EXPORT std::uint32_t roadsim_sumo_bridge_abi();
ROADSIM_SUMO_EXPORT int roadsim_sumo_engine_version(char* output, std::size_t capacity);
ROADSIM_SUMO_EXPORT int roadsim_sumo_engine_revision(char* output, std::size_t capacity);
ROADSIM_SUMO_EXPORT int roadsim_sumo_start(const char* bundle_path,
                                           std::uint64_t root_seed,
                                           std::uint32_t step_length_ms,
                                           char* error,
                                           std::size_t error_capacity);
ROADSIM_SUMO_EXPORT int roadsim_sumo_step(std::uint32_t steps,
                                          std::uint64_t* tick,
                                          char* error,
                                          std::size_t error_capacity);
ROADSIM_SUMO_EXPORT int roadsim_sumo_close(char* error, std::size_t error_capacity);

}
