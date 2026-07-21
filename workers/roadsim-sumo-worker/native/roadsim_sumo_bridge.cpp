#include "roadsim_sumo_bridge.h"

#include <libsumo/Simulation.h>

#include <algorithm>
#include <cstring>
#include <exception>
#include <limits>
#include <string>
#include <vector>

#ifndef ROADSIM_SUMO_VERSION
#error "ROADSIM_SUMO_VERSION must be defined by the exact build"
#endif

#ifndef ROADSIM_SUMO_SOURCE_REVISION
#error "ROADSIM_SUMO_SOURCE_REVISION must be defined by the exact build"
#endif

namespace {

bool active = false;
std::uint64_t current_tick = 0;

int copy_text(const std::string& value, char* output, std::size_t capacity) {
    if (output == nullptr || capacity == 0 || value.size() >= capacity) {
        return 1;
    }
    std::memcpy(output, value.data(), value.size());
    output[value.size()] = '\0';
    return 0;
}

int fail(const char* message, char* error, std::size_t capacity) {
    copy_text(message, error, capacity);
    return 1;
}

void close_after_failed_start() {
    try {
        libsumo::Simulation::close();
    } catch (...) {
    }
    active = false;
    current_tick = 0;
}

std::string runtime_version() {
    const std::string raw = libsumo::Simulation::getVersion().second;
    const auto first_digit = std::find_if(raw.begin(), raw.end(), [](unsigned char value) {
        return value >= '0' && value <= '9';
    });
    if (first_digit == raw.end()) {
        return {};
    }
    const auto end = std::find_if(first_digit, raw.end(), [](unsigned char value) {
        return !((value >= '0' && value <= '9') || value == '.');
    });
    return std::string(first_digit, end);
}

}  // namespace

extern "C" {

std::uint32_t roadsim_sumo_bridge_abi() {
    return 1;
}

int roadsim_sumo_engine_version(char* output, std::size_t capacity) {
    try {
        const std::string version = runtime_version();
        if (version != ROADSIM_SUMO_VERSION) {
            return 1;
        }
        return copy_text(version, output, capacity);
    } catch (...) {
        return 1;
    }
}

int roadsim_sumo_engine_revision(char* output, std::size_t capacity) {
    return copy_text(ROADSIM_SUMO_SOURCE_REVISION, output, capacity);
}

int roadsim_sumo_start(const char* bundle_path,
                       std::uint64_t root_seed,
                       std::uint32_t step_length_ms,
                       char* error,
                       std::size_t error_capacity) {
    if (active || bundle_path == nullptr || step_length_ms == 0 || step_length_ms > 1000) {
        return fail("invalid start state", error, error_capacity);
    }
    try {
        const std::string step_length = std::to_string(step_length_ms / 1000.0);
        const std::vector<std::string> arguments = {
            "sumo",
            "-c",
            bundle_path,
            "--seed",
            std::to_string(root_seed),
            "--step-length",
            step_length,
            "--no-step-log",
            "true",
            "--duration-log.disable",
            "true",
        };
        libsumo::Simulation::start(arguments);
        active = true;
        current_tick = 0;
        return 0;
    } catch (const std::exception&) {
        close_after_failed_start();
        return fail("libsumo start failed", error, error_capacity);
    } catch (...) {
        close_after_failed_start();
        return fail("libsumo start failed", error, error_capacity);
    }
}

int roadsim_sumo_step(std::uint32_t steps,
                      std::uint64_t* tick,
                      char* error,
                      std::size_t error_capacity) {
    if (!active || steps == 0 || steps > 1000000 || tick == nullptr ||
        current_tick > std::numeric_limits<std::uint64_t>::max() - steps) {
        return fail("invalid step state", error, error_capacity);
    }
    try {
        for (std::uint32_t index = 0; index < steps; ++index) {
            libsumo::Simulation::step();
            ++current_tick;
        }
        *tick = current_tick;
        return 0;
    } catch (const std::exception&) {
        return fail("libsumo step failed", error, error_capacity);
    } catch (...) {
        return fail("libsumo step failed", error, error_capacity);
    }
}

int roadsim_sumo_close(char* error, std::size_t error_capacity) {
    if (!active) {
        return fail("invalid close state", error, error_capacity);
    }
    try {
        libsumo::Simulation::close();
        active = false;
        return 0;
    } catch (const std::exception&) {
        return fail("libsumo close failed", error, error_capacity);
    } catch (...) {
        return fail("libsumo close failed", error, error_capacity);
    }
}

}  // extern "C"
