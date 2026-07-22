#include "roadsim_sumo_bridge.h"

#include <libsumo/Simulation.h>
#include <libsumo/Vehicle.h>

#include <algorithm>
#include <charconv>
#include <cmath>
#include <cstring>
#include <exception>
#include <limits>
#include <system_error>
#include <string>
#include <utility>
#include <vector>

#ifndef ROADSIM_SUMO_VERSION
#error "ROADSIM_SUMO_VERSION must be defined by the exact build"
#endif

#ifndef ROADSIM_SUMO_SOURCE_REVISION
#error "ROADSIM_SUMO_SOURCE_REVISION must be defined by the exact build"
#endif

namespace {

static_assert(sizeof(RoadSimVehicleState) == 48, "vehicle ABI layout changed");
static_assert(alignof(RoadSimVehicleState) == 8, "vehicle ABI alignment changed");

bool active = false;
std::uint64_t current_tick = 0;
constexpr char AGENT_PREFIX[] = "rs_agent_";
constexpr double PI = 3.141592653589793238462643383279502884;

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

bool parse_agent_id(const std::string& value, std::uint32_t& output) {
    constexpr std::size_t prefix_length = sizeof(AGENT_PREFIX) - 1;
    if (value.size() <= prefix_length || value.compare(0, prefix_length, AGENT_PREFIX) != 0) {
        return false;
    }
    const char* begin = value.data() + prefix_length;
    const char* end = value.data() + value.size();
    if (end - begin > 1 && *begin == '0') {
        return false;
    }
    const auto result = std::from_chars(begin, end, output);
    return result.ec == std::errc() && result.ptr == end;
}

double mathematical_heading(double navigation_degrees) {
    return std::remainder((90.0 - navigation_degrees) * PI / 180.0, 2.0 * PI);
}

}  // namespace

extern "C" {

std::uint32_t roadsim_sumo_bridge_abi() {
    return 2;
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

int roadsim_sumo_collect_vehicles(RoadSimVehicleState* output,
                                  std::size_t capacity,
                                  std::size_t* output_count,
                                  char* error,
                                  std::size_t error_capacity) {
    if (!active || output_count == nullptr || (output == nullptr && capacity != 0)) {
        return fail("invalid collect state", error, error_capacity);
    }
    try {
        std::vector<std::pair<std::uint32_t, std::string>> vehicles;
        for (const std::string& id : libsumo::Vehicle::getIDList()) {
            std::uint32_t agent_id = 0;
            if (!parse_agent_id(id, agent_id)) {
                return fail("invalid vehicle id", error, error_capacity);
            }
            vehicles.emplace_back(agent_id, id);
        }
        std::sort(vehicles.begin(), vehicles.end());
        if (std::adjacent_find(
                vehicles.begin(),
                vehicles.end(),
                [](const auto& left, const auto& right) { return left.first == right.first; }) !=
            vehicles.end()) {
            return fail("duplicate vehicle id", error, error_capacity);
        }
        *output_count = vehicles.size();
        if (output == nullptr) {
            return 0;
        }
        if (capacity < vehicles.size()) {
            return fail("vehicle buffer too small", error, error_capacity);
        }
        for (std::size_t index = 0; index < vehicles.size(); ++index) {
            const auto& [agent_id, vehicle_id] = vehicles[index];
            const libsumo::TraCIPosition front = libsumo::Vehicle::getPosition(vehicle_id);
            const double heading = mathematical_heading(libsumo::Vehicle::getAngle(vehicle_id));
            const double length = libsumo::Vehicle::getLength(vehicle_id);
            const double width = libsumo::Vehicle::getWidth(vehicle_id);
            const double x = front.x - std::cos(heading) * length * 0.5;
            const double y = front.y - std::sin(heading) * length * 0.5;
            if (!std::isfinite(x) || !std::isfinite(y) || !std::isfinite(heading) ||
                !std::isfinite(length) || !std::isfinite(width) || length <= 0.0 || width <= 0.0) {
                return fail("invalid vehicle state", error, error_capacity);
            }
            output[index] = RoadSimVehicleState{agent_id, x, y, heading, length, width};
        }
        return 0;
    } catch (const std::exception&) {
        return fail("libsumo collect failed", error, error_capacity);
    } catch (...) {
        return fail("libsumo collect failed", error, error_capacity);
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
