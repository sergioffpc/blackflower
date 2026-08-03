#include <Jolt/Jolt.h>

#include <Jolt/ConfigurationString.h>

#include <cstdint>

#if !defined(JPH_CROSS_PLATFORM_DETERMINISTIC)
#error "Blackflower requires Jolt cross-platform deterministic mode"
#endif
#if !defined(BF_JOLT_STRICT_FLOAT) || BF_JOLT_STRICT_FLOAT != 1
#error "Blackflower requires the pinned strict floating-point mode"
#endif

extern "C" const char *bf_jolt_archive_configuration() noexcept {
    return JPH::GetConfigurationString();
}

extern "C" uint32_t bf_jolt_archive_strict_float() noexcept {
    return BF_JOLT_STRICT_FLOAT;
}
