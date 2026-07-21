#include <stdint.h>
#include <stdlib.h>
#include <string.h>

static int active = 0;
static int crash_on_step = 0;
static uint64_t tick = 0;

static int copy_text(const char* value, char* output, size_t capacity) {
    const size_t length = strlen(value);
    if (output == NULL || length >= capacity) {
        return 1;
    }
    memcpy(output, value, length + 1);
    return 0;
}

uint32_t roadsim_sumo_bridge_abi(void) {
    return 1;
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

int roadsim_sumo_close(char* error, size_t error_capacity) {
    (void)error;
    (void)error_capacity;
    if (!active) {
        return 1;
    }
    active = 0;
    return 0;
}
