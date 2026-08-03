#include <Jolt/Jolt.h>

#include <Jolt/ConfigurationString.h>

#if !defined(JPH_CROSS_PLATFORM_DETERMINISTIC)
#error "Blackflower requires Jolt cross-platform deterministic mode"
#endif

extern "C" const char *bf_jolt_archive_configuration() noexcept {
    return JPH::GetConfigurationString();
}
