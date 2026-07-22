#include <stdint.h>
#include <stdlib.h>
#include <string.h>

static int active = 0;
static int crash_on_step = 0;
static int invalid_frame = 0;
static uint64_t tick = 0;

struct RoadSimVehicleState {
    uint32_t agent_id;
    double x_m;
    double y_m;
    double heading_rad;
    double length_m;
    double width_m;
};

static int copy_text(const char* value, char* output, size_t capacity) {
    const size_t length = strlen(value);
    if (output == NULL || length >= capacity) {
        return 1;
    }
    memcpy(output, value, length + 1);
    return 0;
}

uint32_t roadsim_sumo_bridge_abi(void) {
    return 2;
}

int roadsim_sumo_engine_version(char* output, size_t capacity) {
    return copy_text("1.27.1", output, capacity);
}

int roadsim_sumo_engine_revision(char* output, size_t capacity) {
    return copy_text("7717f2379d9e314a0c81c5cec748444de06a2a91", output, capacity);
}

int roadsim_sumo_start(const char* bundle_path,
                       uint64_t root_seed,
                       uint32_t step_length_ms,
                       char* error,
                       size_t error_capacity) {
    (void)root_seed;
    (void)error;
    (void)error_capacity;
    if (active || bundle_path == NULL || step_length_ms == 0) {
        return 1;
    }
    active = 1;
    tick = 0;
    crash_on_step = strstr(bundle_path, "crash") != NULL;
    invalid_frame = strstr(bundle_path, "invalid-frame") != NULL;
    return 0;
}

int roadsim_sumo_step(uint32_t steps,
                      uint64_t* output_tick,
                      char* error,
                      size_t error_capacity) {
    (void)error;
    (void)error_capacity;
    if (!active || steps == 0 || output_tick == NULL) {
        return 1;
    }
    if (crash_on_step) {
        abort();
    }
    tick += steps;
    *output_tick = tick;
    return 0;
}

int roadsim_sumo_collect_vehicles(struct RoadSimVehicleState* output,
                                  size_t capacity,
                                  size_t* output_count,
                                  char* error,
                                  size_t error_capacity) {
    (void)error;
    (void)error_capacity;
    if (!active || output_count == NULL || (output == NULL && capacity != 0)) {
        return 1;
    }
    *output_count = 2;
    if (output == NULL) {
        return 0;
    }
    if (capacity < 2) {
        return 1;
    }
    output[0] = (struct RoadSimVehicleState){7, (double)tick, 2.0, 0.0, 4.5, 1.8};
    output[1] = (struct RoadSimVehicleState){
        invalid_frame ? 7 : 42, (double)tick + 10.0, 3.0, 1.0, 12.0, 2.5};
    return 0;
}

int roadsim_sumo_close(char* error, size_t error_capacity) {
    (void)error;
    (void)error_capacity;
    if (!active) {
        return 1;
    }
    active = 0;
    return 0;
}
